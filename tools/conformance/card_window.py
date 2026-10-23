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

角色预置位（placement，卡 `openmir2-window-character-placement`）：
  * 场景**顶层**可声明 `"placement": {"map": "0122", "x": 30, "y": 32}`，表示「本场景要求
    角色位于 map/x/y」。`card_window.py` 在**每侧的 restore 之后、起栈之前**把该侧测试库
    `characters` 的这一行改到位（C# = `mir2_test_*`，Rust = `mir2_test_*_r`，两侧各改各的库）。
  * **为什么选 DB 预置而不是「走到目标」/GM `@move`**：新角色出生在图 `0`，四个目标 NPC 都不在
    图 0；探针不发跨图移动，`@move` 的 Permission 恒 0 且多张图带 `NORECALL`/`NOPOSITIONMOVE`。
    DB 预置是唯一**确定性**、**起栈前**生效（无竞态）、且只用测试库夹具的做法。
  * **fail-closed**：完成不了就 rc=2 并点名原因（图不在 MapInfo / 坐标越界 / 格子不可走 /
    角色不存在或重名 / `HomeMap` 为空 / 改完与目标不符 / 连带改了别的列）。**绝不让 probe 去超时**——
    开局点在不可走格时 oracle 会随机挪 5 次再回城，客户端只会「点不到 NPC」而不报错。
  * 只写 `MapName`/`CX`/`CY` 三列；预置前后对**其余列做 MD5 指纹对比**，任何其它列变化 => rc=2
    （防止 `characters_ablity.Weight` 那类连带写坏重演）。已登记的口径限制：指纹口径是
    `MD5(CONCAT_WS(CHAR(31), IFNULL(CAST(col AS CHAR), '~')))` —— 字符串值 `'~'` 与 SQL NULL
    在该口径下碰撞（同 `dbsnap` 的 L2 限制族）。本判据要抓的是「非预期的写入」，不是证明
    NULL 语义，故不做逐字节往返；真要逐字节可比时走 `dbsnap` 的快照/restore 路径。
  * 角色库与该侧部署配对：C# = `mir2_test_db`、Rust = `mir2_test_db_r`，并按该侧
    `Server.conf` 的 `[DataBase] database=mir2_test_data*` 交叉核对（读到 `_r` 结尾就要求
    角色库也以 `_r` 结尾）；拿 C# 的库给 Rust 侧预置/拍快照会变成假差异，这里直接 rc=2。
  * 预置的前置条件来自部署内容（只读）：`<envir_root>/Envir/MapInfo.txt` 的图名/服务器号、
    `<envir_root>/Server.conf` 的 `[Share] MapDir` 与 `[Server] ServerIndex`、`<MapDir>/<图>.map`
    的宽高与阻挡位（与 oracle `Envirnoment.CanWalk(x, y, true)` 同判据：越界或 bit15 阻挡 = 不可走）。
    `envir_root` 取 `profile.sides.<side>.envir_root`，缺省由该侧 `binaries[0]` 的目录推出。
  * 预置使用的 MySQL 客户端/账号由 profile 顶层 `db_client` 给出（缺省与 `tools/dbsnap/dbsnap.py`
    同一把 `mysql.exe`、127.0.0.1:3306、root、空口令）；口令只认环境变量
    `CONFORMANCE_DB_PASSWORD`，不写进 profile 文件。
  * `--dry-run` 只打印预置命令并做**离线内容校验**（图/坐标可走），不连库。

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
import struct
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

# --- 角色预置位（placement）----------------------------------------------------
#: MySQL 客户端缺省路径（与 tools/dbsnap/dbsnap.py 的 DEFAULT_MYSQL 同口径）。
DEFAULT_MYSQL = r"D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe"
#: 预置位**只允许**写这三列；其余列必须逐字节不变（指纹比对）。
PLACEMENT_COLUMNS = ("MapName", "CX", "CY")
#: 预置位必须依赖的列（缺任何一个 => 表结构与预期不符，fail-closed）。
PLACEMENT_REQUIRED_COLUMNS = ("Id", "ChrName", "LoginID", "MapName", "CX", "CY",
                              "HomeMap", "Deleted")
#: 图名白名单字符集（varchar(10)；现网 MapInfo.txt 322/322 个图名都落在这个集合里）。
PLACEMENT_MAP_RE = re.compile(r"^[A-Za-z0-9_]{1,10}$")
#: `HUtil32.Separator = {' ', ',', '\t'}`（MapInfo 分段用）。
MAPINFO_SEPS = " \t,"
MAP_HEADER_BYTES = 52
MAP_CELL_BYTES = 12
MAP_WALL_BIT = 0x8000

POWERSHELL = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass"]
CSHARP_STOP = POWERSHELL + [
    "-Command",
    "Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -like "
    "'E:\\tmp\\conformance\\csharp-root\\*' } | ForEach-Object { "
    "Stop-Process -Id $_.ProcessId -Force }",
]

# Rust 栈**实际执行的二进制**（卡 `openmir2-window-rust-binary-evidence`）：
# `rust-root/M2GameSvr/GameSrv.exe` 是同目录 .dll 的 .NET 程序集，从不是被跑的那个文件；
# 证据里的 Rust sha256 必须指向 `mir2-rs/target/{release,debug}/mir2-server.exe`。
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
RUST_SERVER_BIN = os.path.join(
    REPO_ROOT, "mir2-rs", "target", "release", "mir2-server.exe"
).replace(os.sep, "/")
RUST_SERVER_BIN_DEBUG = os.path.join(
    REPO_ROOT, "mir2-rs", "target", "debug", "mir2-server.exe"
).replace(os.sep, "/")
PID_FILE_NAME = "mir2-server.pid.json"


def resolve_rust_binary():
    """release 优先、缺失退 debug（证据里注明用的是 debug）；返回 (path, profile)。"""
    if os.path.exists(RUST_SERVER_BIN):
        return RUST_SERVER_BIN, "release"
    if os.path.exists(RUST_SERVER_BIN_DEBUG):
        return RUST_SERVER_BIN_DEBUG, "debug"
    return RUST_SERVER_BIN, "release(missing)"


def pid_file_path(side_dir, sc):
    """pid 文件路径：**只认 profile 的 `pid_file`**（`{side_dir}` 模板按本侧产物目录展开）。

    部署把 pid 写到 wrapper 的 `-LogDir`（alt = `E:/tmp/conformance/rust-logs`），
    不是 `<side_dir>`；所以路径必须在 profile 里显式声明（load_profile 对
    `require_pid_binaries` 强制校验），代码里不写死任何部署绝对路径。
    """
    raw = sc.get("pid_file")
    if not raw:
        return os.path.join(side_dir, PID_FILE_NAME)
    return str(raw).replace("{side_dir}", side_dir)


def verify_pid_binaries(side_dir, sc, side):
    """起栈后核对 `mir2-server.pid.json` 的 `roles[].exe` 与 profile 登记的 binaries。

    返回 `(ok, error_or_none, info)`；不一致/缺文件 ⇒ 调用方把该侧判 rc=2 并点名。
    """
    pid_path = pid_file_path(side_dir, sc)
    norm = lambda p: os.path.normcase(os.path.normpath(str(p).replace("/", os.sep)))
    registered = [norm(p) for p in (sc.get("binaries") or [])]
    info = {"pid_file": pid_path, "registered": list(sc.get("binaries") or []), "roles": []}
    if not os.path.exists(pid_path):
        return False, ("缺少 %s：无法核对 %s 栈实际执行的 exe（拒绝只记 sha256 继续跑）"
                       % (PID_FILE_NAME, side)), info
    try:
        # **必须 utf-8-sig**：pid 文件由 `start_mir2_server.ps1` 用 Windows PowerShell 5
        # 的 `Set-Content -Encoding utf8` 写出，**带 UTF-8 BOM**；用 utf-8 读会抛
        # `Unexpected UTF-8 BOM` ⇒ 真窗口永远 rc=2（review B2）。
        with open(pid_path, "r", encoding="utf-8-sig") as fh:
            data = json.load(fh)
    except (OSError, ValueError) as ex:
        return False, ("读取 %s 失败：%s" % (PID_FILE_NAME, ex)), info
    roles = data.get("roles") if isinstance(data, dict) else None
    if not roles:
        return False, ("%s 没有 roles[]：无法核对 %s 栈实际执行的 exe" % (PID_FILE_NAME, side)), info
    bad = []
    for role in roles:
        exe = role.get("exe") if isinstance(role, dict) else None
        info["roles"].append({"role": role.get("role") if isinstance(role, dict) else None,
                              "exe": exe})
        if not exe or norm(exe) not in registered:
            bad.append("%s=%s" % (role.get("role") if isinstance(role, dict) else "?",
                                  exe or "(空)"))
    if bad:
        return False, ("pid 文件的 exe 与 profile binaries 不一致：%s（登记 %s）"
                       % (", ".join(bad), ", ".join(sc.get("binaries") or []))), info
    return True, None, info


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
        "db_client": {"mysql": [DEFAULT_MYSQL], "host": "127.0.0.1", "port": 3306,
                      "user": "root", "password": ""},
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
                "envir_root": "E:/tmp/conformance/csharp-root/M2GameSvr",
                "db": {"preset": "test", "characters_db": "mir2_test_db"},
            },
            "rust": {
                "label": "Rust",
                "stack": POWERSHELL + ["-File", "E:/tmp/conformance/start-rust.ps1"],
                "stop": POWERSHELL + ["-File", "E:/tmp/conformance/stop-rust.ps1"],
                "restore": ["bash", "E:/tmp/conformance/restore-baseline-r.sh"],
                "ports": [15000, 15100, 15500, 15600, 15700, 16000, 17000, 17100, 17600],
                "ready_ports": [15000, 17000, 17100, 17600],
                "probe_extra": ["--game-gate", "127.0.0.1:17600"],
                "binaries": [RUST_SERVER_BIN],
                "require_pid_binaries": True,
                # 部署把 pid 写到 wrapper 的 `-LogDir`（`start-rust.ps1` 里写死
                # `E:/tmp/conformance/rust-logs`），**不是** `<side_dir>`（review B1）。
                # 下一次真窗口读的就是这个文件；路径只在这里声明，代码里不写死。
                "pid_file": "E:/tmp/conformance/rust-logs/mir2-server.pid.json",
                "envir_root": "E:/tmp/conformance/rust-root/M2GameSvr",
                "db": {"tables": RUST_DB_TABLES,
                       "characters_db": "mir2_test_db_r",
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
        "db_client": {"mysql": [DEFAULT_MYSQL], "host": "127.0.0.1", "port": 3306,
                      "user": "root", "password": ""},
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
                "db": {"preset": "prod", "characters_db": "mir2_db"},
            },
            "rust": {
                "label": "Rust（默认端口，需部署在 E:/tmp/conformance/rust-default-root）",
                "stack": POWERSHELL + ["-File", "E:/tmp/conformance/start-rust-default.ps1"],
                "stop": POWERSHELL + ["-File", "E:/tmp/conformance/stop-rust-default.ps1"],
                "restore": ["bash", "E:/tmp/conformance/restore-baseline.sh"],
                "ports": [5000, 5100, 5500, 5600, 5700, 6000, 7000, 7100, 7200],
                "ready_ports": [5000, 7000, 7100, 7200],
                "probe_extra": [],
                "binaries": [RUST_SERVER_BIN],
                "require_pid_binaries": True,
                # 该部署的 wrapper 目前不在仓内；约定它必须用
                # `-LogDir $env:CARD_WINDOW_SIDE_DIR` 把 pid 写进本侧产物目录。
                # 写在别处 ⇒ 起栈后 rc=2 并点名实际要找的文件（fail-closed）。
                "pid_file": "{side_dir}/" + PID_FILE_NAME,
                "db": {"preset": "test", "characters_db": "mir2_test_db"},
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
        elif isinstance(a, str) and a.startswith("--password="):
            out[i] = "--password=***"
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


def validate_db_client(value, where):
    """`db_client`（预置位用的 MySQL 客户端参数）加载期校验；None 视为未配置。"""
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ConfigError("%s 必须是对象（mysql/host/port/user/password）" % where)
    bad_keys = set(value) - {"mysql", "host", "port", "user", "password"}
    if bad_keys:
        raise ConfigError("%s 有未知键 %s（fail-closed）" % (where, sorted(bad_keys)))
    if value.get("mysql") is not None:
        validate_cmd_argv(value["mysql"], "%s.mysql" % where)
    for key in ("host", "user", "password"):
        v = value.get(key)
        if v is not None and not isinstance(v, str):
            raise ConfigError("%s.%s 必须是字符串" % (where, key))
    port = value.get("port")
    if port is not None and (not isinstance(port, int) or isinstance(port, bool)
                             or not (0 < port < 65536)):
        raise ConfigError("%s.port 必须是 1..65535 的整数" % where)
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
        cdb = db.get("characters_db")
        if cdb is not None and (not isinstance(cdb, str) or not cdb):
            raise ConfigError("profile.sides.%s.db.characters_db 必须是非空字符串（库名）"
                              % side)
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
        pf = sc.get("pid_file")
        if pf is not None and (not isinstance(pf, str) or not pf):
            raise ConfigError("profile.sides.%s.pid_file 必须是非空字符串（pid 文件路径，"
                              "可用 {side_dir} 占位）" % side)
        if sc.get("require_pid_binaries") and not pf:
            # 卡 openmir2-window-rust-binary-evidence / review B1：pid 文件由部署的
            # `-LogDir` 决定，**必须**在 profile 里显式声明，不许代码里兜底写死。
            raise ConfigError(
                "profile.sides.%s 开了 require_pid_binaries 就必须显式声明 pid_file"
                "（部署把 pid 写到 wrapper 的 -LogDir，不是 <side_dir>）" % side)
        sx = sc.get("server_exe")
        if sx is not None and (not isinstance(sx, str) or not sx):
            raise ConfigError("profile.sides.%s.server_exe 必须是非空字符串" % side)
        env = sc.get("env")
        if env is not None:
            if not isinstance(env, dict) or any(
                    not isinstance(k, str) or not isinstance(v, str) for k, v in env.items()):
                raise ConfigError("profile.sides.%s.env 必须是字符串到字符串的对象" % side)
        er = sc.get("envir_root")
        if er is not None and (not isinstance(er, str) or not er):
            raise ConfigError("profile.sides.%s.envir_root 必须是非空字符串（部署内容根目录）"
                              % side)
    validate_db_client(prof.get("db_client"), "profile.db_client")
    for side in SIDES:
        validate_db_client(sides[side].get("db_client"),
                           "profile.sides.%s.db_client" % side)
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
        if db.get("characters_db") is not None:
            specs.append((str(db["characters_db"]),
                          "sides.%s.db.characters_db" % side))
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
        texts.append(str(db.get("characters_db") or ""))
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

    def run(self, side, step, argv, env=None, timeout=600, stdin_text=None,
            capture=False):
        """跑一步命令。`capture=True` => 返回 (rc, stdout, stderr)；否则只返回 rc。

        `stdin_text` 用于把 SQL 喂给 mysql 客户端（不经命令行，避免引号/注入歧义）。
        """
        safe = _redact_argv(argv)
        self.commands.append({"side": side, "step": step, "argv": safe})
        self.steps.append("%s:%s" % (side, step))
        self.log("== %s/%s :: %s" % (side, step, _join_argv(safe)))
        if stdin_text and self.log_path and not self.dry_run:
            with open(self.log_path, "a", encoding="utf-8", newline="\n") as f:
                f.write("---- %s/%s stdin ----\n%s\n" % (side, step, stdin_text))
        if self.dry_run:
            return (0, "", "") if capture else 0
        try:
            kw = {}
            if stdin_text is not None:
                kw["input"] = stdin_text
            p = subprocess.run(argv, capture_output=True, text=True, encoding="utf-8",
                               errors="replace", timeout=timeout,
                               env=env, cwd=REPO_ROOT, **kw)
            rc = p.returncode
            out = p.stdout or ""
            err = p.stderr or ""
        except subprocess.TimeoutExpired as e:
            rc = 124
            out = ""
            err = "TIMEOUT after %ss\n%s\n%s" % (
                timeout, e.stdout or "", e.stderr or "")
        except OSError as e:
            rc = 127
            out, err = "", "OSError: %s" % e
        if out or err:
            with open(self.log_path, "a", encoding="utf-8", newline="\n") as f:
                f.write("---- %s/%s rc=%s ----\n%s%s\n" % (side, step, rc, out, err))
        self.log("   %s/%s rc=%s" % (side, step, rc))
        return (rc, out, err) if capture else rc


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
            # 场景 `computed` 声明（computed 类变量：derived_from + delta）必须透传给 derive，
            # 否则 move_walk 的 move_x/move_y 会 fail-closed（安全但跑不起来）。
            "computed": dict(d.get("computed") or {}),
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


def scenario_computed(path):
    """读场景声明的 `computed`（computed 类变量的派生声明）；坏文件/坏声明 => ConfigError（rc=2）。

    校验口径只认 `param_discovery.validate_computed`（单一口径，不在本工具另造一套）。
    """
    try:
        with open(path, "r", encoding="utf-8") as fh:
            doc = json.load(fh)
    except (OSError, ValueError) as ex:
        raise ConfigError("%s: cannot load scenario: %s" % (path, ex))
    if not isinstance(doc, dict):
        raise ConfigError("%s: scenario must be a JSON object" % path)
    try:
        return param_discovery.validate_computed(doc.get("computed"),
                                                 doc.get("required") or [], path)
    except param_discovery.DiscoveryError as ex:
        raise ConfigError(str(ex))


def missing_reason(var):
    """缺变量原因：直接复用 param_discovery 自己登记的说明（不在本工具另造一套口径）。"""
    return (param_discovery.REASONS.get(var)
            or param_discovery.UNDERIVABLE.get(var)
            or "规则表无此变量（不猜）")


def derive_from_frames(frames_path, required, computed=None):
    """从本侧 discovery 帧推导 required。坏输入 => param_discovery.DiscoveryError（rc=2）。"""
    frames = param_discovery.load_frames(frames_path)
    return param_discovery.derive(frames, required, computed=computed)


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


# ---------------------------------------------------------------------------
# 角色预置位（placement）：restore 之后、起栈之前把测试角色放到目标图/坐标
#
# 选型（卡 openmir2-window-character-placement）：
#   (a) DB 改测试库 characters.MapName/CX/CY   <= 本实现
#   (b) 在 discovery/probe 前加「走到目标」步骤  —— 不可行：探针不发跨图移动，出生图 0 与
#       四个目标 NPC 所在图之间没有客户端可发的跨图指令；走图还会注入 RNG/时序。
#   (c) GM `@move`                                —— 不可行：Permission 恒 0，且多图带
#       NORECALL/NOPOSITIONMOVE。
# 预置必须发生在 restore 之后（restore 会把库复位）且起栈之前（角色记录只在登录时读一次）。
# ---------------------------------------------------------------------------
def scenario_placement(path):
    """读场景顶层的 `placement` 声明；没有 => None；形状非法 => ConfigError(fail-closed)。"""
    try:
        with open(path, "r", encoding="utf-8-sig") as f:
            data = json.load(f)
    except (OSError, ValueError) as e:
        raise ConfigError("%s: cannot load scenario: %s" % (path, e))
    if not isinstance(data, dict):
        raise ConfigError("%s: scenario must be a JSON object" % path)
    pl = data.get("placement")
    if pl is None:
        return None
    if not isinstance(pl, dict):
        raise ConfigError("%s: scenario.placement 必须是对象（map/x/y）" % path)
    bad_keys = set(pl) - {"map", "x", "y", "note"}
    if bad_keys:
        raise ConfigError("%s: scenario.placement 有未知键 %s（fail-closed）"
                          % (path, sorted(bad_keys)))
    mp = pl.get("map")
    if not isinstance(mp, str) or not mp:
        raise ConfigError("%s: scenario.placement.map 必须是非空字符串" % path)
    if not PLACEMENT_MAP_RE.match(mp):
        raise ConfigError(
            "%s: scenario.placement.map=%r 非法（只允许 [A-Za-z0-9_]{1,10}："
            "characters.MapName 是 varchar(10)，越界会被截断/注入）" % (path, mp))
    for axis in ("x", "y"):
        v = pl.get(axis)
        if not isinstance(v, int) or isinstance(v, bool):
            raise ConfigError("%s: scenario.placement.%s 必须是整数（实际 %r）"
                              % (path, axis, v))
        if not (0 <= v <= 0xFFFF):
            raise ConfigError("%s: scenario.placement.%s=%r 越界（0..65535）"
                              % (path, axis, v))
    if pl.get("note") is not None and not isinstance(pl["note"], str):
        raise ConfigError("%s: scenario.placement.note 必须是字符串" % path)
    return {"map": mp, "x": int(pl["x"]), "y": int(pl["y"]),
            "note": pl.get("note") or ""}


def _get_valid_str3(text):
    """`HUtil32.GetValidStr3(source, ref dest, Separator)` 的 (dest, 返回值) 二元组。

    C#：`dest = 第一段`，**返回第二段**（`Split(div, 2, RemoveEmptyEntries)`）。
    本项目 MapInfo 的行首不会有多余分隔符，这里按「跳过前导分隔符后取第一段/其余」实现。
    """
    s = str(text)
    i = 0
    n = len(s)
    while i < n and s[i] in MAPINFO_SEPS:
        i += 1
    j = i
    while j < n and s[j] not in MAPINFO_SEPS:
        j += 1
    first = s[i:j]
    k = j
    while k < n and s[k] in MAPINFO_SEPS:
        k += 1
    return first, s[k:]


def _split_map_name_and_file(name):
    """`MapManager.AddMapInfo` 的逻辑名/文件名拆分（`MapManager.cs:64-86`）。"""
    s = str(name)
    if "|" in s:
        return tuple(_get_valid_str3(s))  # dest=逻辑名, 返回值=文件名
    if "<" in s and ">" in s and s.index("<") < s.rindex(">"):
        inner = s[s.index("<") + 1:s.rindex(">")]
        rest = s[:s.index("<")] + s[s.rindex(">") + 1:]
        if inner:
            return rest, inner
    return s, s


def read_ini_sections(path):
    """极简 INI 读取（`Server.conf`：`[Section]` + `key=value`，`;`/`#` 注释）。

    读不到/坏行 => 不抛异常，缺项由调用方按默认值处理（避免把内容格式差异变成假红）。
    """
    out = {}
    try:
        with open(path, "rb") as f:
            raw = f.read()
    except OSError:
        return out
    text = raw.decode("utf-8-sig", "replace")
    section = None
    for line in text.splitlines():
        s = line.strip()
        if not s or s[0] in ";#":
            continue
        if s.startswith("[") and "]" in s:
            section = s[1:s.index("]")].strip().lower()
            out.setdefault(section, {})
            continue
        if section is None or "=" not in s:
            continue
        k, v = s.split("=", 1)
        out[section].setdefault(k.strip().lower(), v.strip())
    return out


def read_map_info(envir_dir):
    """解析 `<Envir>/MapInfo.txt`（含一层 `ConnectMapInfo <file>` 子文件）。

    返回 {逻辑图名: {"file": 地图文件名, "server_index": int}}；重名保持首条（oracle 同样丢弃后者）。
    """
    path = os.path.join(envir_dir, "MapInfo.txt")
    try:
        with open(path, "rb") as f:
            raw = f.read()
    except OSError as e:
        raise ConfigError("读不了 MapInfo：%s（%s）" % (path, e))
    text = raw.decode("gbk", "replace")
    maps = {}
    for line in text.splitlines():
        # `HUtil32.CompareLStr("ConnectMapInfo", line)`：前缀比较（大小写不敏感）。
        if line[:14].lower() == "connectmapinfo":
            _first, sub = _get_valid_str3(line)
            if sub:
                sub_name = sub.split()[0]
                sub_path = os.path.join(envir_dir, sub_name)
                maps.update(_parse_map_info_file(sub_path))
            continue
        if not line.startswith("[") or "]" not in line:
            continue
        inner = line[1:line.index("]")]
        name_spec, desc = _get_valid_str3(inner)
        if desc.startswith('"') and desc.count('"') >= 2:
            desc = desc[1:desc.index('"', 1)]
        _desc_first, s4c = _get_valid_str3(desc)
        try:
            server_index = int((s4c or "").strip() or 0)
        except ValueError:
            server_index = 0
        map_name, map_file = _split_map_name_and_file(name_spec)
        if not map_name:
            continue
        maps.setdefault(map_name, {"file": map_file, "server_index": server_index})
    return maps


def _parse_map_info_file(path):
    """`ConnectMapInfo` 子文件的 section 解析（与 MapInfo.txt 同一语法）。"""
    maps = {}
    try:
        with open(path, "rb") as f:
            raw = f.read()
    except OSError:
        return maps
    for line in raw.decode("gbk", "replace").splitlines():
        if not line.startswith("[") or "]" not in line:
            continue
        inner = line[1:line.index("]")]
        name_spec, desc = _get_valid_str3(inner)
        if desc.startswith('"') and desc.count('"') >= 2:
            desc = desc[1:desc.index('"', 1)]
        _desc_first, s4c = _get_valid_str3(desc)
        try:
            server_index = int((s4c or "").strip() or 0)
        except ValueError:
            server_index = 0
        map_name, map_file = _split_map_name_and_file(name_spec)
        if map_name:
            maps.setdefault(map_name, {"file": map_file, "server_index": server_index})
    return maps


def read_map_cell(map_path, x, y):
    """读 `.map` 的宽高与 (x,y) 格；返回 dict(width, height, walkable, bk, mid, fr)。

    判据与 oracle 登录期 `Envirnoment.CanWalk(x, y, true)` 一致：**越界或 bit15 阻挡 = 不可走**
    （`boFlag=true` 时不看格上对象，所以「NPC 站同一格」不影响登录落点）。
    """
    try:
        with open(map_path, "rb") as f:
            header = f.read(MAP_HEADER_BYTES)
            if len(header) < MAP_HEADER_BYTES:
                raise ConfigError("地图文件太短（< %d 字节）：%s"
                                  % (MAP_HEADER_BYTES, map_path))
            width, height = struct.unpack_from("<hh", header, 0)
            if width <= 0 or height <= 0:
                raise ConfigError("地图文件宽高非法（%dx%d）：%s" % (width, height, map_path))
            if x < 0 or y < 0 or x >= width or y >= height:
                return {"width": width, "height": height, "in_bounds": False,
                        "walkable": False, "bk": None, "mid": None, "fr": None}
            f.seek(MAP_HEADER_BYTES + (x * height + y) * MAP_CELL_BYTES)
            cell = f.read(MAP_CELL_BYTES)
    except OSError as e:
        raise ConfigError("读不了地图文件 %s：%s" % (map_path, e))
    if len(cell) < MAP_CELL_BYTES:
        raise ConfigError("地图文件被截断（格 %d,%d 读不到）：%s" % (x, y, map_path))
    bk, mid, fr = struct.unpack_from("<HHH", cell, 0)
    return {"width": width, "height": height, "in_bounds": True,
            "walkable": not (bk & MAP_WALL_BIT) and not (fr & MAP_WALL_BIT),
            "bk": bk, "mid": mid, "fr": fr}


def placement_envir_root(side, sc):
    """部署内容根目录：`sides.<side>.envir_root`，缺省由 `binaries[0]` 的目录推出。"""
    er = sc.get("envir_root")
    if er:
        return os.path.abspath(er)
    bins = sc.get("binaries") or []
    if not bins:
        raise ConfigError("profile.sides.%s 缺 binaries/envir_root：无法定位 MapInfo/Map" % side)
    return os.path.dirname(os.path.abspath(bins[0]))


def placement_characters_db(side, sc):
    """该侧角色库名：必须是测试库（`db.characters_db`，缺省按 preset/tables 推）。"""
    db = sc.get("db") or {}
    name = db.get("characters_db")
    if not name:
        preset = str(db.get("preset") or "")
        if preset in ("test", "prod"):
            name = {"test": "mir2_test_db", "prod": "mir2_db"}[preset]
        else:
            for t in db.get("tables") or []:
                if str(t).endswith(".characters"):
                    name = str(t).split(".", 1)[0]
                    break
    if not name:
        raise ConfigError(
            "profile.sides.%s 缺 db.characters_db：预置位必须知道改哪个库的角色表"
            "（C# 用 mir2_test_db、Rust 用 mir2_test_db_r，两套库不同名）" % side)
    kind = classify_db_name(name)
    if kind == "prod":
        raise ConfigError("profile.sides.%s.db.characters_db=%r 是生产库：预置位只许写测试库"
                          % (side, name))
    if kind != "test":
        raise ConfigError("profile.sides.%s.db.characters_db=%r 无法判定为测试库（保守拒绝）"
                          % (side, name))
    return name


def placement_db_client(prof, side):
    """合并 `profile.db_client` / `sides.<side>.db_client` / 环境变量，得到 mysql 调用参数。"""
    cfg = dict(prof.get("db_client") or {})
    cfg.update((prof["sides"][side] or {}).get("db_client") or {})
    mysql = cfg.get("mysql") or [DEFAULT_MYSQL]
    if not isinstance(mysql, list) or not mysql:
        raise ConfigError("db_client.mysql 必须是非空 argv 数组")
    host = os.environ.get("CONFORMANCE_DB_HOST") or cfg.get("host") or "127.0.0.1"
    user = os.environ.get("CONFORMANCE_DB_USER") or cfg.get("user") or "root"
    password = os.environ.get("CONFORMANCE_DB_PASSWORD")
    if password is None:
        password = cfg.get("password") or ""
    port = cfg.get("port") or 3306
    return {"mysql": [str(x) for x in mysql], "host": str(host), "port": int(port),
            "user": str(user), "password": str(password)}


def _sql_quote(value):
    """把值转成 MySQL 字符串字面量（转义 `\\ ' \\0 \\n \\r \\Z`）。"""
    out = []
    for ch in str(value):
        if ch == "\\":
            out.append("\\\\")
        elif ch == "'":
            out.append("\\'")
        elif ch == "\0":
            out.append("\\0")
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\r":
            out.append("\\r")
        elif ch == "\x1a":
            out.append("\\Z")
        else:
            out.append(ch)
    return "'" + "".join(out) + "'"


def _placement_row_sql(db, chr_name, login_id, columns, by_id=None):
    """读一行的：命中行数 / Id / MapName / CX / CY / 其余列 MD5 指纹 / HomeMap / Deleted。

    `by_id` 给了就按主键读（读回用），否则按 ChrName+LoginID 读（定位用）。
    """
    fp_cols = [c for c in columns if c not in PLACEMENT_COLUMNS]
    fp = ", CHAR(31), ".join("IFNULL(CAST(`%s` AS CHAR), '~')" % c for c in fp_cols)
    where = ("`Id` = %d" % int(by_id)) if by_id is not None else (
        "`ChrName` = %s AND `LoginID` = %s"
        % (_sql_quote(chr_name), _sql_quote(login_id)))
    return (
        "SELECT COUNT(*), COALESCE(MIN(`Id`), 0), COALESCE(MIN(`MapName`), ''),\n"
        "       COALESCE(MIN(`CX`), -1), COALESCE(MIN(`CY`), -1),\n"
        "       COALESCE(MIN(`_fp`), ''), COALESCE(MIN(`_home`), ''),\n"
        "       COALESCE(MIN(`_deleted`), 0)\n"
        "FROM (SELECT `Id`, `MapName`, `CX`, `CY`, `HomeMap` AS `_home`,\n"
        "             IFNULL(`Deleted`, 0) AS `_deleted`,\n"
        "             MD5(CONCAT_WS(%s)) AS `_fp`\n"
        "      FROM `%s`.`characters` WHERE %s) AS `t`;\n"
        % (fp, db, where))


def _parse_placement_row(out):
    """解析 `_placement_row_sql` 的 8 列输出；形状不对 => ConfigError。"""
    lines = [ln for ln in (out or "").splitlines() if ln.strip()]
    if len(lines) != 1:
        raise ConfigError("预置位读行结果不是 1 行（实际 %d 行）：%r" % (len(lines), out[:300]))
    parts = lines[0].split("\t")
    if len(parts) != 8:
        raise ConfigError("预置位读行结果的列数不是 8（实际 %d）：%r"
                          % (len(parts), lines[0][:300]))
    try:
        return {"count": int(parts[0]), "id": int(parts[1]), "map": parts[2],
                "x": int(parts[3]), "y": int(parts[4]), "fp": parts[5],
                "home_map": parts[6], "deleted": int(parts[7] or 0)}
    except ValueError as e:
        raise ConfigError("预置位读行结果无法解析：%r（%s）" % (lines[0][:300], e))


def _check_db_pairing(side, envir_root, chars_db):
    """C#/Rust 两套库不同名：用部署 `Server.conf` 的 [DataBase] database 交叉核对配对。

    只在部署侧写的是 `mir2_test_data*` 时判定（alt 剖面就是这样），其它情况不判（避免假红）。
    """
    ini = read_ini_sections(os.path.join(envir_root, "Server.conf"))
    dsn = (ini.get("database") or {}).get("connctionstring") or ""
    m = DB_IN_CONN_RE.search(dsn)
    if not m:
        return None
    data_db = m.group(1)
    if not data_db.startswith("mir2_test_"):
        return None
    family = "rust" if data_db.endswith("_r") else "csharp"
    expected = "mir2_test_db_r" if family == "rust" else "mir2_test_db"
    if chars_db != expected:
        raise ConfigError(
            "profile.sides.%s.db.characters_db=%r 与该侧部署不配对：%s 的 Server.conf "
            "[DataBase] database=%s ⇒ 这一侧的角色库必须是 %r（两侧库不同名，拿错库"
            "预置/拍快照都会变成假差异）"
            % (side, chars_db, os.path.join(envir_root, "Server.conf"), data_db, expected))
    return {"data_db": data_db, "expected_characters_db": expected}


def validate_placement_content(side, prof_side, placement):
    """离线校验（只读部署内容）：图存在、服务器号匹配、坐标在界内且可走。"""
    envir_root = placement_envir_root(side, prof_side)
    ini = read_ini_sections(os.path.join(envir_root, "Server.conf"))
    share = ini.get("share") or {}
    server_sec = ini.get("server") or {}
    envir_dir = os.path.join(envir_root, share.get("envirdir") or "Envir")
    map_dir = os.path.join(envir_root, share.get("mapdir") or "Map")
    map_info_path = os.path.join(envir_dir, "MapInfo.txt")
    if not os.path.isfile(map_info_path):
        raise ConfigError(
            "场景声明了 placement 但找不到 MapInfo：%s（envir_root=%s；图名/坐标无法校验，"
            "拒绝盲改库）" % (map_info_path, envir_root))
    try:
        server_index = int(str(server_sec.get("serverindex") or "0"))
    except ValueError:
        server_index = 0
    maps = read_map_info(envir_dir)
    name = placement["map"]
    entry = maps.get(name)
    if entry is None:
        raise ConfigError(
            "placement.map=%r 不在 %s 里（oracle GetMapInfo 会返回 null ⇒ 直接切服/登不进）"
            % (name, map_info_path))
    if int(entry.get("server_index") or 0) != server_index:
        raise ConfigError(
            "placement.map=%r 在 MapInfo 里属于服务器号 %s，而这一侧 ServerIndex=%s："
            "oracle 会把它当别的服并发送切服（角色落不了地）"
            % (name, entry.get("server_index"), server_index))
    map_path = os.path.join(map_dir, str(entry.get("file") or name) + ".map")
    if not os.path.isfile(map_path):
        raise ConfigError("placement.map=%r 的地图文件不存在：%s" % (name, map_path))
    cell = read_map_cell(map_path, placement["x"], placement["y"])
    if not cell["in_bounds"]:
        raise ConfigError(
            "placement 坐标越界：%r (%d,%d) 不在 %dx%d 的 %s 内"
            % (name, placement["x"], placement["y"], cell["width"], cell["height"],
               os.path.basename(map_path)))
    if not cell["walkable"]:
        raise ConfigError(
            "placement 坐标不可走：%r (%d,%d) 是阻挡格（bk=%#06x fr=%#06x）。"
            "oracle 登录期 CanWalk(x,y,true) 为假时会随机挪 5 次再回落到 HomeMap —— "
            "探针只会点不到 NPC、不会报错，所以这里 fail-closed"
            % (name, placement["x"], placement["y"], cell["bk"], cell["fr"]))
    return {"envir_root": envir_root, "envir_dir": envir_dir, "map_info": map_info_path,
            "map_file": map_path, "map_size": [cell["width"], cell["height"]],
            "server_index": server_index,
            "map_server_index": int(entry.get("server_index") or 0)}


def _mysql_argv(client):
    argv = list(client["mysql"]) + [
        "-h", client["host"], "-P", str(client["port"]), "-u", client["user"],
        "--password=%s" % client["password"],
        "--default-character-set=utf8", "--batch", "--raw", "--skip-column-names",
    ]
    return argv


def run_placement(runner, side, prof, sc, placement, env):
    """在一侧执行预置位：restore 之后、起栈之前。返回证据 dict（rc=0 才算完成）。

    fail-closed 的口径见模块头：任何一步不达预期都 rc=2 并点名，**绝不带着错位继续 probe**。
    """
    res = {
        "side": side,
        "declared": {"map": placement["map"], "x": placement["x"], "y": placement["y"],
                     "note": placement.get("note") or ""},
        "source": "scenario",
        "db": None,
        "db_pairing": None,
        "envir_root": None,
        "envir_dir": None,
        "map_info": None,
        "map_file": None,
        "map_size": None,
        "server_index": None,
        "map_server_index": None,
        "action": None,
        "before": None,
        "after": None,
        "changed_columns": [],
        "sql": {"read": None, "update": None},
        "rc": None,
        "error": None,
    }
    try:
        info = validate_placement_content(side, sc, placement)
        res.update(dict((k, info[k]) for k in (
            "envir_root", "envir_dir", "map_info", "map_file", "map_size",
            "server_index", "map_server_index")))
        db = placement_characters_db(side, sc)
        res["db"] = db
        res["db_pairing"] = _check_db_pairing(side, info["envir_root"], db)
        client = placement_db_client(prof, side)
    except ConfigError as e:
        res["error"] = "预置位前置校验失败：%s" % e
        res["rc"] = 2
        return res

    argv = _mysql_argv(client)
    timeout = 120

    def sql(tag, text):
        rc, out, err = runner.run(side, tag, argv, env=env, timeout=timeout,
                                  stdin_text=text, capture=True)
        if rc != 0:
            raise ConfigError("%s 执行失败（mysql rc=%s）：%s"
                              % (tag, rc, (err or out or "").strip()[:400]))
        return out

    chr_name = str(prof.get("char_name") or "")
    login_id = str(prof.get("account") or "")
    if not chr_name or not login_id:
        res["error"] = ("预置位需要 profile 的 char_name/account（定位 characters 行）；"
                        "当前 char_name=%r account=%r" % (chr_name, login_id))
        res["rc"] = 2
        return res

    try:
        cols_out = sql("placement.columns",
                       "SHOW COLUMNS FROM `%s`.`characters`;\n" % db)
        columns = [ln.split("\t")[0] for ln in cols_out.splitlines() if ln.strip()]
        missing = [c for c in PLACEMENT_REQUIRED_COLUMNS if c not in columns]
        if missing:
            raise ConfigError(
                "%s.characters 缺列 %s：表结构与预期不符（预置位不改表结构，只改 "
                "MapName/CX/CY，因此直接拒绝）" % (db, missing))
        read_sql = _placement_row_sql(db, chr_name, login_id, columns)
        res["sql"]["read"] = read_sql.strip()
        before = _parse_placement_row(sql("placement.read", read_sql))
        res["before"] = {"id": before["id"], "map": before["map"], "x": before["x"],
                         "y": before["y"], "fingerprint": before["fp"],
                         "home_map": before["home_map"]}
        if before["count"] == 0:
            raise ConfigError(
                "预置位找不到角色：%s.characters WHERE ChrName=%r AND LoginID=%r 命中 0 行"
                % (db, chr_name, login_id))
        if before["count"] > 1:
            raise ConfigError(
                "预置位定位不唯一：%s.characters 命中 %d 行（ChrName=%r LoginID=%r）——"
                "拒绝猜哪一行" % (db, before["count"], chr_name, login_id))
        if not before["home_map"]:
            raise ConfigError(
                "角色 %r 的 HomeMap 为空：oracle 登录期会用 HomeMap + Random 覆盖落点，"
                "预置位无效（先把 HomeMap 补上，不能靠 MAP 预置蒙混）" % chr_name)
        if before["deleted"] == 1:
            raise ConfigError("角色 %r 的 Deleted=1（已删除）：select 不到，先修夹具"
                              % chr_name)

        target = (placement["map"], placement["x"], placement["y"])
        if (before["map"], before["x"], before["y"]) == target:
            res["action"] = "noop"
        else:
            upd = ("UPDATE `%s`.`characters` SET `MapName` = %s, `CX` = %d, `CY` = %d\n"
                   " WHERE `Id` = %d;\nSELECT ROW_COUNT();\n"
                   % (db, _sql_quote(target[0]), target[1], target[2], before["id"]))
            res["sql"]["update"] = upd.strip()
            out = sql("placement.update", upd).strip()
            try:
                affected = int((out.splitlines() or ["-1"])[0])
            except ValueError:
                raise ConfigError("placement.update 的 ROW_COUNT 不可解析：%r" % out[:200])
            if affected != 1:
                raise ConfigError(
                    "placement.update 影响了 %d 行（期望 1）：角色行可能在预置期间被删/改，"
                    "拒绝继续" % affected)
            res["action"] = "updated"

        after = _parse_placement_row(sql("placement.verify", read_sql))
        res["after"] = {"id": after["id"], "map": after["map"], "x": after["x"],
                        "y": after["y"], "fingerprint": after["fp"]}
        if after["count"] != 1 or after["id"] != before["id"]:
            raise ConfigError(
                "预置位读回异常：命中 %d 行、Id=%s（期望 1 行 Id=%s）"
                % (after["count"], after["id"], before["id"]))
        if (after["map"], after["x"], after["y"]) != target:
            raise ConfigError(
                "预置位读回与目标不符：期望 %r (%d,%d)，实际 %r (%d,%d)"
                % (target[0], target[1], target[2], after["map"], after["x"], after["y"]))
        if after["fp"] != before["fp"]:
            raise ConfigError(
                "预置位连带改了**其它列**（MapName/CX/CY 之外的 MD5 指纹变了：%s -> %s）："
                "只允许改这三列，拒绝带着被写坏的字段继续"
                % (before["fp"][:12] or "<空>", after["fp"][:12] or "<空>"))
        res["changed_columns"] = ["MapName", "CX", "CY"] if res["action"] == "updated" else []
        res["rc"] = 0
        return res
    except ConfigError as e:
        res["error"] = "预置位失败：%s" % e
        res["rc"] = 2
        return res


def preview_placement(runner, side, prof, sc, placement, ctx):
    """`--dry-run` 用：只做离线内容校验 + 打印将要执行的命令（不连库/不改库）。"""
    out = {"side": side, "declared": dict(placement), "rc": None, "error": None,
           "commands": [], "offline_check": None}
    try:
        info = validate_placement_content(side, sc, placement)
        db = placement_characters_db(side, sc)
        _check_db_pairing(side, info["envir_root"], db)
        client = placement_db_client(prof, side)
    except ConfigError as e:
        out["error"] = "预置位前置校验失败（真跑会 rc=2）：%s" % e
        out["rc"] = 2
        return out, True
    out["offline_check"] = {"envir_root": info["envir_root"], "map_file": info["map_file"],
                            "map_size": info["map_size"],
                            "server_index": info["server_index"],
                            "map_server_index": info["map_server_index"], "db": db}
    argv = _mysql_argv(client)
    chr_name = str(prof.get("char_name") or "")
    login_id = str(prof.get("account") or "")
    out["commands"] = [
        {"argv": _redact_argv(argv), "stdin": "SHOW COLUMNS FROM `%s`.`characters`;" % db},
        {"argv": _redact_argv(argv),
         "stdin": "SELECT ... FROM `%s`.`characters` WHERE ChrName=%r AND LoginID=%r;"
                  % (db, chr_name, login_id)},
        {"argv": _redact_argv(argv),
         "stdin": ("UPDATE `%s`.`characters` SET MapName=%r, CX=%d, CY=%d WHERE Id=<row>;"
                   % (db, placement["map"], placement["x"], placement["y"]))},
        {"argv": _redact_argv(argv), "stdin": "SELECT ... （读回校验：目标值 + 其余列指纹）"},
    ]
    runner.log("[%s] 预置位（dry-run 不连库）：map=%s x=%d y=%d db=%s 图=%s %dx%d 可走"
               % (side, placement["map"], placement["x"], placement["y"], db,
                  os.path.basename(info["map_file"]), info["map_size"][0],
                  info["map_size"][1]))
    out["rc"] = 0
    return out, False




def run_side(runner, side, prof, ctx, password, placement=None):
    sc = prof["sides"][side]
    side_dir = ctx["side_dir"]
    db_dir = ctx["db_dir"]
    res = {
        "side": side,
        "label": sc.get("label") or side,
        "restore_rc": None,
        "placement": None,
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

        # 1b) 角色预置位（声明了才跑）：restore 之后、起栈之前；完成不了 => rc=2，
        #     **绝不让 probe 带着错位去超时**（点不到 NPC 时探针不会报错，只会等超时）。
        if placement:
            pres = run_placement(runner, side, prof, sc, placement, env)
            try:
                write_json_file(os.path.join(side_dir, "placement.json"), pres)
            except OSError as e:
                pres["error"] = ((pres.get("error") or "") + "；写 placement.json 失败：%s"
                                 % e).strip("；")
                pres["rc"] = 2
            res["placement"] = pres
            if pres.get("rc") != 0:
                res["error"] = pres.get("error") or "角色预置位失败"
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

        # 3b) **卡 `openmir2-window-rust-binary-evidence`**：先按 release/debug 解析 Rust 栈
        # 真正执行的二进制，再拿 `mir2-server.pid.json` 的 `roles[].exe` 对账；不符 ⇒ rc=2，
        # 不许「只记一个错的 sha256 继续跑」。
        if sc.get("require_pid_binaries"):
            # profile 可显式声明 `server_exe`（窗口从别的检出/worktree 跑时用，review M1）；
            # 缺省按本仓 release → debug 解析。
            if sc.get("server_exe"):
                path, kind = str(sc["server_exe"]), "profile(server_exe)"
            else:
                path, kind = resolve_rust_binary()
            res["binary_profile"] = kind
            if not os.path.exists(path):
                res["error"] = ("Rust 侧二进制不存在：%s（%s）——请先 `cargo build -p mir2-server "
                                "--release`，或在 profile 显式给 server_exe（不拿不存在的文件"
                                "记 sha256）" % (path, kind))
                return res
            sc["binaries"] = [path]
            ok, err, info = verify_pid_binaries(side_dir, sc, side)
            res["pid_binaries"] = info
            if not ok:
                res["error"] = err
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
                                                         disco.get("required") or [],
                                                         disco.get("computed") or {})
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
    plc = summary.get("placement") or {}
    if plc.get("enabled"):
        d = plc.get("declared") or {}
        lines.append("placement: map=%s x=%s y=%s (scenario 顶层声明)"
                     % (d.get("map"), d.get("x"), d.get("y")))
        for side in SIDES:
            p = (plc.get("sides") or {}).get(side) or {}
            if not p:
                lines.append("  %-6s placement (not run)" % side)
                continue
            b = p.get("before") or {}
            a = p.get("after") or {}
            lines.append("  %-6s placement rc=%s db=%s action=%s "
                         "before=%s(%s,%s) after=%s(%s,%s) fp=%s->%s"
                         % (side, p.get("rc"), p.get("db"), p.get("action"),
                            b.get("map"), b.get("x"), b.get("y"),
                            a.get("map"), a.get("x"), a.get("y"),
                            (b.get("fingerprint") or "-")[:12],
                            (a.get("fingerprint") or "-")[:12]))
            if p.get("error"):
                lines.append("         error=%s" % p["error"])
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
        lines.append("  %-6s rc=%s restore=%s placement=%s stack=%s ready_missing=%s "
                     "discover=%s probe=%s db=%s stop=%s frames=%s"
                     % (side, r.get("rc"), r.get("restore_rc"),
                        (r.get("placement") or {}).get("rc"), r.get("stack_rc"),
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
    computed = {}
    placement = None
    if scenario and os.path.isfile(scenario):
        required = scenario_required(scenario)
        computed = scenario_computed(scenario)
        placement = scenario_placement(scenario)
    discover = bool(cfg.get("discover"))
    discovery_scenario = os.path.abspath(
        cfg.get("discovery_scenario") or DEFAULT_DISCOVERY_SCENARIO)
    needs_discovery = discover or bool(required)
    discovery_cfg = {"needed": needs_discovery, "forced": discover,
                     "scenario": discovery_scenario, "required": required,
                     "computed": computed,
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
        "placement": {
            "enabled": bool(placement),
            "declared": placement,
            "sides": {},
            "note": ("场景顶层 placement => 每侧 restore 之后、起栈之前把该侧测试库 "
                     "characters 的 MapName/CX/CY 改到位；完成不了 rc=2（不让 probe 超时）"
                     if placement else None),
        },
        "discovery": {
            "enabled": needs_discovery,
            "forced": discover,
            "scenario": discovery_scenario if needs_discovery else None,
            "scenario_sha256": (sha256_file(discovery_scenario)
                                if needs_discovery and os.path.isfile(discovery_scenario)
                                else None),
            "required": list(required),
            "computed": dict(computed),
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
            side_failed = False
            runner.run(side, "restore", expand_argv(sc["restore"], ctx))
            if placement:
                pres, failed = preview_placement(runner, side, prof, sc, placement, ctx)
                summary["placement"]["sides"][side] = pres
                if failed:
                    runner.log("[%s] 预置位不可行 ⇒ 真跑 rc=2（不起栈、不跑场景）：%s"
                               % (side, pres.get("error")))
                    preview_failed = True
                    side_failed = True
            if not side_failed:
                runner.log("[%s] 端口预检：%s" % (side, sc.get("ports") or []))
                runner.log("[%s] 就绪等待：%s（timeout %ss）"
                           % (side, sc.get("ready_ports") or [],
                              prof.get("ready_timeout_s", 120)))
                runner.run(side, "stack", expand_argv(sc["stack"], ctx))
            vars_argv = []
            if needs_discovery and not side_failed:
                dctx = discovery_ctx(ctx)
                disc_argv = build_probe_argv(sc, dctx, prof, password)
                runner.run(side, "discover", disc_argv)
                pattern = cfg.get("discovery_frames")
                if required and pattern:
                    frames_path = expand_argv([str(pattern)], dctx)[0]
                    try:
                        values, missing = derive_from_frames(
                            frames_path, required,
                            ctx["discovery"].get("computed") or {})
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
                "dry-run 预览：预置位/参数发现不可行 ⇒ 真跑会 rc=2（命令序列到 stop 为止）")
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
            res = run_side(runner, side, prof, ctx, password, placement=placement)
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
        if res.get("placement"):
            # README/card-window.json 的 placement 段读这份（与 discovery 同口径）
            summary["placement"]["sides"][side] = res["placement"]
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
    if mode == "mysql":
        # 角色预置位的 db 桩：读 stdin 的 SQL，按语句形状返回 mysql --batch 口径的 TSV。
        # 状态落 `<log>.placement-<side>`，模拟「库里的那一行」；环境变量可以造各种失败。
        import re as _re
        sql_text = sys.stdin.read()
        state_path = log + ".placement-" + side
        st = {"map": "0", "x": 289, "y": 618, "fp": "fp-before"}
        init = os.environ.get("STUB_PLACEMENT_INIT")
        if init:
            parts = init.split(",")
            st.update({"map": parts[0], "x": int(parts[1]), "y": int(parts[2])})
        if os.path.exists(state_path):
            try:
                with open(state_path, encoding="utf-8") as f:
                    st = json.load(f)
            except (OSError, ValueError):
                pass
        if "SHOW COLUMNS" in sql_text:
            print("\n".join("%s\tint(11)\tYES\t\tNULL\t" % c for c in (
                ["Id", "ServerIndex", "LoginID", "ChrName", "MapName", "CX", "CY",
                 "Level", "HomeMap", "Deleted"]
                if not os.environ.get("STUB_PLACEMENT_NO_HOMECOL")
                else ["Id", "ServerIndex", "LoginID", "ChrName", "MapName", "CX", "CY",
                      "Level", "Deleted"])))
            return 0
        if "UPDATE" in sql_text:
            mm = _re.search(r"`MapName`\s*=\s*'([^']*)'", sql_text)
            mx = _re.search(r"`CX`\s*=\s*(-?\d+)", sql_text)
            my = _re.search(r"`CY`\s*=\s*(-?\d+)", sql_text)
            if os.environ.get("STUB_PLACEMENT_NOWRITE") != side:
                st["map"] = mm.group(1) if mm else st["map"]
                st["x"] = int(mx.group(1)) if mx else st["x"]
                st["y"] = int(my.group(1)) if my else st["y"]
                st["fp"] = ("fp-corrupt"
                            if os.environ.get("STUB_PLACEMENT_CORRUPT") == side
                            else st.get("fp", "fp-before"))
            with open(state_path, "w", encoding="utf-8") as f:
                json.dump(st, f, sort_keys=True)
            print(os.environ.get("STUB_PLACEMENT_AFFECTED", "1"))
            return 0
        # 读行
        if os.environ.get("STUB_PLACEMENT_MISSING") == side:
            print("0\t0\t\t-1\t-1\t\t\t0")
            return 0
        home = "" if os.environ.get("STUB_PLACEMENT_NOHOME") == side else "0"
        deleted = "1" if os.environ.get("STUB_PLACEMENT_DELETED") == side else "0"
        print("1\t7\t%s\t%d\t%d\t%s\t%s\t%s"
              % (st["map"], st["x"], st["y"], st.get("fp", "fp-before"), home, deleted))
        with open(state_path, "w", encoding="utf-8") as f:
            json.dump(st, f, sort_keys=True)
        return 0
    if mode == "stack":
        if os.environ.get("STUB_STACK_FAIL") == side:
            return 3
        pid_mode = os.environ.get("STUB_PID_MODE")
        if pid_mode and side == "rust":
            # 模拟 start_mir2_server.ps1：把 roles[].exe 写进 `-LogDir` 的 pid 文件。
            # 真文件由 Windows PowerShell 5 的 Set-Content -Encoding utf8 写出 => 带 BOM。
            if pid_mode != "missing":
                exe = os.environ.get("STUB_PID_EXE", "")
                if pid_mode == "wrong":
                    exe = "E:/tmp/conformance/rust-root/M2GameSvr/GameSrv.exe"
                side_dir = os.environ.get("CARD_WINDOW_SIDE_DIR") or "."
                enc = "utf-8-sig" if os.environ.get("STUB_PID_BOM") == "1" else "utf-8"
                with open(os.path.join(side_dir, "mir2-server.pid.json"), "w",
                          encoding=enc) as f:
                    json.dump({"server_exe": exe,
                               "roles": [{"role": "game-svr", "pid": 4242,
                                          "exe": exe}]}, f)
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
            if os.environ.get("STUB_DISCOVERY_SELF_X"):
                self_x = int(os.environ["STUB_DISCOVERY_SELF_X"])
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
                      busy_port=None, ready_ports=None, envir_roots=None,
                      chars_db=None, pid_wiring=None, pid_bom=False):
    py = sys.executable

    def side(name):
        db = {"command": [py, stub, "db", name, log, "{db_dir}"],
              "databases": ["mir2_test_db"],
              "characters_db": (chars_db or {}).get(
                  name, "mir2_test_db" if name == "csharp" else "mir2_test_db_r")}
        prof_side = {
            "label": name,
            "stack": [py, stub, "stack", name, log],
            "stop": [py, stub, "stop", name, log],
            "restore": [py, stub, "restore", name, log],
            "probe": [py, stub, "probe", name, log],
            "db": db,
            "db_client": {"mysql": [py, stub, "mysql", name, log]},
            "ports": [],
            "ready_ports": [],
            "probe_extra": [],
            "binaries": [stub],
        }
        if envir_roots:
            prof_side["envir_root"] = envir_roots[name]
        if name == "csharp" and (busy_port or ready_ports):
            prof_side["ports"] = [busy_port] if busy_port else []
            prof_side["ready_ports"] = list(ready_ports or [])
        if name == "rust" and mutate_rust:
            prof_side["probe_extra"] = ["--mutate"]
        if name == "csharp" and stack_fail == "csharp":
            prof_side["env"] = {"STUB_STACK_FAIL": "csharp"}
        if name == "rust" and pid_wiring:
            # 接线用例：stub 栈写 pid 文件，走完整 run_side（review I1）。
            # server_exe 显式覆盖 => 不依赖本检出是否构建了 target/{release,debug}。
            prof_side["require_pid_binaries"] = True
            prof_side["server_exe"] = stub
            prof_side["pid_file"] = "{side_dir}/" + PID_FILE_NAME
            prof_side["env"] = {"STUB_PID_MODE": pid_wiring, "STUB_PID_EXE": stub,
                                "STUB_PID_BOM": "1" if pid_bom else "0"}
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


def same_path(a, b):
    """Windows 口径的路径比较（大小写/分隔符/相对片段归一）。"""
    def n(p):
        return os.path.normcase(os.path.normpath(str(p).replace("/", os.sep)))
    return n(a) == n(b)


def _selftest_pid_binaries(tmp):
    """卡 openmir2-window-rust-binary-evidence：pid 文件 exe 与登记 binaries 的一致性。

    纯函数层用例（含 review B1/B2 的牙）：① 一致 ⇒ ok；② **带 BOM 的真 pid 文件** ⇒ ok
    （PowerShell 5 `Set-Content -Encoding utf8` 写出的就是带 BOM 的）；③ 不符 ⇒ 失败并点名
    角色；④ 缺文件 ⇒ 失败并点名；⑤ 路径只认 profile 的 `pid_file`（不是 `<side_dir>`）；
    ⑥ `{side_dir}` 模板；⑦ 牙：登记指回 `rust-root/GameSrv.exe` ⇒ 红；
    ⑧ 内置 alt profile 必须显式声明真窗口读的那个部署路径（牙：改回旧位置 ⇒ 红）。
    """
    d = os.path.join(tmp, "pidbin")
    os.makedirs(d, exist_ok=True)
    exe = os.path.join(d, "mir2-server.exe")
    pid = os.path.join(d, PID_FILE_NAME)
    sc = {"binaries": [exe.replace(os.sep, "/")]}
    out = []

    def write_pid(path, exe_value, encoding="utf-8"):
        with open(path, "w", encoding=encoding) as f:
            json.dump({"server_exe": exe_value,
                       "roles": [{"role": "game-svr", "pid": 4242, "exe": exe_value}]}, f)

    # ① 正常登记 ⇒ ok
    write_pid(pid, exe)
    ok, err, _ = verify_pid_binaries(d, sc, "rust")
    out.append((ok and err is None, "pid 文件 exe 与登记一致 ⇒ ok"))
    # ② 真 pid 文件带 UTF-8 BOM（PowerShell 5 Set-Content -Encoding utf8）⇒ 仍能读
    write_pid(pid, exe, encoding="utf-8-sig")
    ok, err, _ = verify_pid_binaries(d, sc, "rust")
    out.append((ok and err is None, "带 UTF-8 BOM 的 pid 文件 ⇒ 仍 ok（utf-8-sig 读取）"))
    # ③ 登记与 pid 不符 ⇒ False 且点名
    write_pid(pid, os.path.join(d, "GameSrv.exe"))
    ok, err, _ = verify_pid_binaries(d, sc, "rust")
    out.append((not ok and err and "game-svr" in err, "pid exe 与登记不符 ⇒ 失败并点名角色"))
    # ④ pid 文件缺失 ⇒ False（rc=2 路径）
    try:
        os.remove(pid)
    except OSError:
        pass
    ok, err, _ = verify_pid_binaries(d, sc, "rust")
    out.append((not ok and err and PID_FILE_NAME in err, "缺 pid 文件 ⇒ 失败且点名文件"))
    # ⑤ pid 路径只认 profile 声明（不是 <side_dir>）：真部署写在 wrapper 的 -LogDir
    logs = os.path.join(tmp, "pidbin-logs")
    os.makedirs(logs, exist_ok=True)
    real_pid = os.path.join(logs, PID_FILE_NAME)
    write_pid(real_pid, exe)
    empty_side = os.path.join(tmp, "pidbin-side-empty")
    os.makedirs(empty_side, exist_ok=True)
    ok, err, info = verify_pid_binaries(empty_side, {"binaries": [exe],
                                                     "pid_file": real_pid}, "rust")
    out.append((ok and err is None and same_path(info.get("pid_file"), real_pid),
                "pid 路径只认 profile 的 pid_file（不是 <side_dir>）"))
    # ⑥ `{side_dir}` 模板按本侧产物目录展开
    ok, err, info = verify_pid_binaries(
        logs, {"binaries": [exe], "pid_file": "{side_dir}/" + PID_FILE_NAME}, "rust")
    out.append((ok and err is None and same_path(info.get("pid_file"), real_pid),
                "pid_file 的 {side_dir} 模板按本侧目录展开"))
    # ⑦ 牙：把 registrations 指回 rust-root 的 .NET 程序集（老值）⇒ 对拍变红
    write_pid(pid, exe)
    stale = {"binaries": ["E:/tmp/conformance/rust-root/M2GameSvr/GameSrv.exe"]}
    ok, _, _ = verify_pid_binaries(d, stale, "rust")
    out.append((not ok, "牙：登记指回 rust-root/GameSrv.exe ⇒ 一致性断言红"))
    # ⑧ 内置 alt profile 显式声明真窗口要读的那个文件（review B1）
    expected = "E:/tmp/conformance/rust-logs/" + PID_FILE_NAME
    try:
        alt = load_profile("alt")["sides"]["rust"]
        resolved = pid_file_path(os.path.join(tmp, "side"), alt)
        declared_ok = same_path(resolved, expected)
    except ConfigError:
        declared_ok = False
    out.append((declared_ok,
                "内置 alt profile 显式声明 pid 文件 = 部署 -LogDir（%s）" % expected))
    return out


def _selftest_pid_wiring(tmp, base_cfg, profile_factory):
    """接线覆盖（review I1 / 牙③）：stub 栈写 pid 文件，走完整 `run_side` 端到端。

    纯函数层绿不代表接线活着——旁路 `if sc.get("require_pid_binaries"):` 整段时，
    下面三条必须变红（否则这条卡的核心行为随时可以是死的）。
    """
    def run(tag, mode, bom=False):
        prof = profile_factory(pid_wiring=mode, pid_bom=bom)
        summary = execute(base_cfg(os.path.join(tmp, "pid-wiring-" + tag), prof))
        return summary, (summary.get("sides") or {}).get("rust") or {}

    out = []
    s_ok, r_ok = run("ok", "ok", bom=True)
    info = r_ok.get("pid_binaries") or {}
    out.append((s_ok["exit_code"] == 0 and r_ok.get("rc") == 0
                and bool(info.get("roles"))
                and str(info.get("pid_file") or "").endswith(PID_FILE_NAME),
                "接线：起栈后读 pid 文件（带 BOM）对账通过 => 整体 rc=0 且证据记下 roles[]"))
    s_bad, r_bad = run("bad", "wrong")
    err_bad = r_bad.get("error") or ""
    out.append((s_bad["exit_code"] == 2 and r_bad.get("rc") == 2
                and "不一致" in err_bad and "game-svr" in err_bad,
                "接线：pid exe 与登记不符 => run_side rc=2 并点名角色（整体 rc=2）"))
    s_miss, r_miss = run("miss", "missing")
    err_miss = r_miss.get("error") or ""
    out.append((s_miss["exit_code"] == 2 and r_miss.get("rc") == 2
                and PID_FILE_NAME in err_miss,
                "接线：缺 pid 文件 => run_side rc=2 且点名文件（整体 rc=2）"))
    return out


def selftest():
    tmp = tempfile.mkdtemp(prefix="card-window-selftest-")
    stub = os.path.join(tmp, "stub.py")
    with open(stub, "w", encoding="utf-8", newline="\n") as f:
        f.write(STUB_SOURCE)
    log = os.path.join(tmp, "order.log")
    scenario = os.path.join(tmp, "scenario.json")
    with open(scenario, "w", encoding="utf-8") as f:
        f.write("{}\n")

    # --- 角色预置位（placement）的离线夹具：两侧各一份部署内容 + 合成 .map ---
    def write_map(path, w, h, walls=()):
        body = bytearray()
        for x in range(w):
            for y in range(h):
                bk = MAP_WALL_BIT if (x, y) in walls else 0
                body += struct.pack("<HHH", bk, 0, 0) + b"\0" * 6
        with open(path, "wb") as f:
            f.write(struct.pack("<hh", w, h) + b"\0" * (MAP_HEADER_BYTES - 4))
            f.write(bytes(body))

    def make_deploy(root, data_db):
        os.makedirs(os.path.join(root, "Envir"), exist_ok=True)
        os.makedirs(os.path.join(root, "Map"), exist_ok=True)
        with open(os.path.join(root, "Envir", "MapInfo.txt"), "w",
                  encoding="gbk", newline="\n") as f:
            # `[NAME DESC SERVERINDEX]`：服务器号是描述的第 2 段（与 oracle 同口径）
            f.write("[0 主城]\n[0122 皇宫]\n[GA2 情人家园]\n[N109 桃源源]\n"
                    "[WALL 墙图]\n[ELSEWHERE 别的服 3]\n")
        with open(os.path.join(root, "Server.conf"), "w",
                  encoding="utf-8", newline="\n") as f:
            f.write("[DataBase]\nDbType=MySQL\n"
                    "ConnctionString=server=127.0.0.1;uid=root;pwd=;database=%s;\n\n"
                    "[Server]\nServerIndex=0\n\n[Share]\nEnvirDir=Envir\nMapDir=Map\n"
                    % data_db)
        write_map(os.path.join(root, "Map", "0.map"), 20, 20)
        write_map(os.path.join(root, "Map", "0122.map"), 30, 30)
        write_map(os.path.join(root, "Map", "GA2.map"), 10, 10)
        write_map(os.path.join(root, "Map", "N109.map"), 10, 10)
        write_map(os.path.join(root, "Map", "WALL.map"), 10, 10, walls={(5, 5)})
        write_map(os.path.join(root, "Map", "ELSEWHERE.map"), 10, 10)

    deploy = {"csharp": os.path.join(tmp, "deploy-csharp"),
              "rust": os.path.join(tmp, "deploy-rust")}
    make_deploy(deploy["csharp"], "mir2_test_data")
    make_deploy(deploy["rust"], "mir2_test_data_r")

    def place_scenario(pl, name="place"):
        path = os.path.join(tmp, "scen-%s-%d.json" % (name, len(os.listdir(tmp))))
        with open(path, "w", encoding="utf-8") as f:
            json.dump({"name": name, "placement": pl,
                       "steps": [{"wait_for": 658, "timeout_ms": 10}]}, f)
        return path

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

        # DISC-9（卡 openmir2-window-discovery-rules）：场景的 computed 声明必须透传到 derive，
        #   computed 类变量按「已推导量 + 声明 delta」算出并真的进 --var。
        #   桩发现帧两侧 self_x 默认 7/8（DISC-1..5 用差异），这里把两侧钉成 7，
        #   让 move_x=self_x+1 两侧都算成 8（非会话变量跨侧必须相等）。
        scen_comp = os.path.join(tmp, "scen-computed.json")
        with open(scen_comp, "w", encoding="utf-8") as f:
            json.dump({"name": "computed", "required": ["move_x", "move_y"],
                       "computed": {"move_x": {"derived_from": "self_x", "delta": 1},
                                    "move_y": {"derived_from": "self_y", "delta": 0}},
                       "steps": [{"wait_for": 50, "timeout_ms": 10}]}, f)
        prof_comp = _selftest_profile(tmp, stub, log)
        prof_comp["sides"]["csharp"]["env"] = {"STUB_DISCOVERY_SELF_X": "7"}
        prof_comp["sides"]["rust"]["env"] = {"STUB_DISCOVERY_SELF_X": "7"}
        fresh_log()
        out_comp = os.path.join(tmp, "run-computed")
        s_comp = execute(base_cfg(out_comp, prof_comp, scenario=scen_comp))
        chk(s_comp["exit_code"] == 0 and s_comp["verdict"] == "PASS",
            "DISC-9 computed 场景两侧同起点 => rc=0/PASS（实际 %s/%s）"
            % (s_comp["exit_code"], s_comp["verdict"]))
        argv_cc = probe_argv_of(out_comp, "csharp")
        argv_cr = probe_argv_of(out_comp, "rust")
        chk("move_x=8" in argv_cc and "move_y=12" in argv_cc,
            "DISC-9 csharp 的 probe 命令带 computed --var move_x=8/move_y=12（实际 %s）"
            % argv_cc[-6:])
        chk("move_x=8" in argv_cr and "move_y=12" in argv_cr,
            "DISC-9 rust 的 probe 命令带 computed --var move_x=8/move_y=12（实际 %s）"
            % argv_cr[-6:])
        try:
            with open(os.path.join(out_comp, "csharp", "discovered.json"),
                      encoding="utf-8") as f:
                d_comp = json.load(f)
        except (OSError, ValueError):
            d_comp = {}
        val = (d_comp.get("values") or {}).get("move_x") or {}
        chk(val.get("kind") == "computed"
            and (val.get("source") or {}).get("delta") == 1,
            "DISC-9 discovered.json 落盘 kind=computed + delta（%s）"
            % {k: val.get(k) for k in ("value", "kind")})

        # computed 声明坏掉（缺 delta）=> 加载期 rc=2，绝不静默按 0 算
        scen_bad = os.path.join(tmp, "scen-computed-bad.json")
        with open(scen_bad, "w", encoding="utf-8") as f:
            json.dump({"name": "computed-bad", "required": ["move_x"],
                       "computed": {"move_x": {"derived_from": "self_x"}},
                       "steps": []}, f)
        bad_refused = False
        try:
            execute(base_cfg(os.path.join(tmp, "run-computed-bad"),
                             _selftest_profile(tmp, stub, log), scenario=scen_bad,
                             dry_run=True))
        except ConfigError as e:
            bad_refused = "delta" in str(e)
        chk(bad_refused, "DISC-9 computed 缺 delta => ConfigError（加载期 fail-closed）")

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

        # 18) 角色预置位（placement，卡 openmir2-window-character-placement）：
        #     场景顶层声明 => 每侧 restore 之后、**起栈之前**把该侧测试库改到位；
        #     完成不了 => rc=2 点名，且不起栈/不跑场景（绝不让 probe 去超时）。
        def place_prof(**kw):
            kw.setdefault("envir_roots", deploy)
            return _selftest_profile(tmp, stub, log, **kw)

        def reset_place_state():
            fresh_log()
            for s in SIDES:
                try:
                    os.remove(log + ".placement-" + s)
                except OSError:
                    pass

        scen_place = place_scenario({"map": "0122", "x": 10, "y": 10})

        # 18a) 已就位 => 无操作（不执行 UPDATE）
        reset_place_state()
        prof_p0 = place_prof()
        for s in SIDES:
            prof_p0["sides"][s]["env"] = {"STUB_PLACEMENT_INIT": "0122,10,10"}
        s_p0 = execute(base_cfg(os.path.join(tmp, "run-place-noop"), prof_p0,
                                scenario=scen_place))
        p0 = (s_p0["sides"].get("csharp") or {}).get("placement") or {}
        chk(s_p0["exit_code"] == 0 and p0.get("action") == "noop",
            "PL-1 已就位 => 不执行 UPDATE（action=%r，rc=%s）"
            % (p0.get("action"), s_p0["exit_code"]))
        chk("placement.update" not in [c["step"] for c in s_p0["commands"]],
            "PL-1 已就位 => 命令序列里没有 placement.update（%s）"
            % [c["step"] for c in s_p0["commands"]][:8])

        # 18b) 需移动 => 预置成功后才起栈/跑场景；两侧各改各的库，读回等于目标
        reset_place_state()
        out_p1 = os.path.join(tmp, "run-place-move")
        s_p1 = execute(base_cfg(out_p1, place_prof(), scenario=scen_place))
        steps_p1 = [(c["side"], c["step"]) for c in s_p1["commands"]
                    if c["side"] == "csharp"]
        chk(s_p1["exit_code"] == 0 and s_p1["verdict"] == "PASS",
            "PL-2 需移动 => 预置后 rc=0/PASS（实际 %s/%s）"
            % (s_p1["exit_code"], s_p1["verdict"]))
        chk([x[1] for x in steps_p1][:5]
            == ["restore", "placement.columns", "placement.read", "placement.update",
                "placement.verify"]
            and "stack" not in [x[1] for x in steps_p1][:5],
            "PL-2 预置发生在 restore 之后、起栈之前（%s）"
            % [x[1] for x in steps_p1][:6])
        cs_p1 = (s_p1["sides"].get("csharp") or {}).get("placement") or {}
        rs_p1 = (s_p1["sides"].get("rust") or {}).get("placement") or {}
        chk(cs_p1.get("action") == "updated"
            and (cs_p1.get("after") or {}).get("map") == "0122"
            and (cs_p1.get("after") or {}).get("x") == 10
            and (cs_p1.get("after") or {}).get("y") == 10,
            "PL-2 C# 侧预置后读回 = 目标位（%s）" % (cs_p1.get("after"),))
        chk(cs_p1.get("db") == "mir2_test_db" and rs_p1.get("db") == "mir2_test_db_r",
            "PL-3 两侧各改各的测试库（csharp=%r rust=%r）"
            % (cs_p1.get("db"), rs_p1.get("db")))
        chk((cs_p1.get("before") or {}).get("fingerprint")
            == (cs_p1.get("after") or {}).get("fingerprint")
            and cs_p1.get("changed_columns") == ["MapName", "CX", "CY"],
            "PL-4 只改 MapName/CX/CY：其余列 MD5 指纹不变（%s）"
            % cs_p1.get("changed_columns"))
        try:
            with open(os.path.join(out_p1, "csharp", "placement.json"),
                      encoding="utf-8") as f:
                pj = json.load(f)
        except (OSError, ValueError):
            pj = {}
        chk(pj.get("rc") == 0 and pj.get("source") == "scenario"
            and bool(pj.get("sql", {}).get("update")),
            "PL-5 每侧落 placement.json（含 SQL 与前后值，%s）"
            % sorted(pj)[:6])
        try:
            with open(os.path.join(out_p1, "README.txt"), encoding="utf-8") as f:
                readme_p1 = f.read()
        except OSError:
            readme_p1 = ""
        chk("placement: map=0122 x=10 y=10" in readme_p1
            and "csharp placement rc=0 db=mir2_test_db action=updated" in readme_p1
            and "placement rc=0 db=mir2_test_db_r action=updated" in readme_p1
            and "placement (not run)" not in readme_p1,
            "PL-5 README 的 placement 段写了两侧的前后值（不是 (not run)）")

        # 18c) 目标图不在 MapInfo => rc=2 点名词，且不起栈/不跑场景
        reset_place_state()
        s_p2 = execute(base_cfg(os.path.join(tmp, "run-place-badmap"), place_prof(),
                                scenario=place_scenario({"map": "NOPE", "x": 1, "y": 1},
                                                        "badmap")))
        err_p2 = (s_p2["sides"].get("csharp") or {}).get("error") or ""
        chk(s_p2["exit_code"] == 2 and "NOPE" in err_p2,
            "PL-6 目标图不存在 => rc=2 点名图名（%r）" % err_p2[:90])
        chk("stack csharp" not in read_log() and "probe csharp" not in read_log(),
            "PL-6 预置失败 => 不起栈、不跑场景（不让 probe 去超时）")

        # 18d) 坐标越界 / 落在阻挡格 => rc=2 点名
        reset_place_state()
        s_p3 = execute(base_cfg(os.path.join(tmp, "run-place-oob"), place_prof(),
                                scenario=place_scenario({"map": "0122", "x": 100,
                                                         "y": 10}, "oob")))
        err_p3 = (s_p3["sides"].get("csharp") or {}).get("error") or ""
        chk(s_p3["exit_code"] == 2 and "越界" in err_p3 and "30x30" in err_p3,
            "PL-7 坐标越界 => rc=2 点名图/坐标/尺寸（%r）" % err_p3[:90])
        reset_place_state()
        s_p4 = execute(base_cfg(os.path.join(tmp, "run-place-wall"), place_prof(),
                                scenario=place_scenario({"map": "WALL", "x": 5, "y": 5},
                                                        "wall")))
        err_p4 = (s_p4["sides"].get("csharp") or {}).get("error") or ""
        chk(s_p4["exit_code"] == 2 and "不可走" in err_p4,
            "PL-8 目标格是墙 => rc=2（oracle 会随机挪位/回城，探针只会超时，%r）"
            % err_p4[:90])
        reset_place_state()
        s_p5 = execute(base_cfg(os.path.join(tmp, "run-place-else"),
                                place_prof(),
                                scenario=place_scenario({"map": "ELSEWHERE", "x": 1,
                                                         "y": 1}, "else")))
        err_p5 = (s_p5["sides"].get("csharp") or {}).get("error") or ""
        chk(s_p5["exit_code"] == 2 and "服务器号" in err_p5,
            "PL-9 图属于别的服务器号 => rc=2（oracle 会切服，%r）" % err_p5[:90])

        # 18e) 连同改坏其它列 => rc=2 点名「其它列」
        reset_place_state()
        prof_p6 = place_prof()
        prof_p6["sides"]["csharp"]["env"] = {"STUB_PLACEMENT_CORRUPT": "csharp"}
        s_p6 = execute(base_cfg(os.path.join(tmp, "run-place-corrupt"), prof_p6,
                                scenario=scen_place))
        err_p6 = (s_p6["sides"].get("csharp") or {}).get("error") or ""
        chk(s_p6["exit_code"] == 2 and "其它列" in err_p6,
            "PL-10 预置连带改坏其它列 => rc=2（%r）" % err_p6[:90])

        # 18f) 角色不存在 / HomeMap 为空 / 已删除 => rc=2 点名
        for tag, env, want in (
                ("missing", {"STUB_PLACEMENT_MISSING": "csharp"}, "命中 0 行"),
                ("nohome", {"STUB_PLACEMENT_NOHOME": "csharp"}, "HomeMap"),
                ("deleted", {"STUB_PLACEMENT_DELETED": "csharp"}, "Deleted=1")):
            reset_place_state()
            prof_x = place_prof()
            prof_x["sides"]["csharp"]["env"] = env
            s_x = execute(base_cfg(os.path.join(tmp, "run-place-" + tag), prof_x,
                                   scenario=scen_place))
            err_x = (s_x["sides"].get("csharp") or {}).get("error") or ""
            chk(s_x["exit_code"] == 2 and want in err_x,
                "PL-11 预置前置不满足（%s）=> rc=2 点名（%r）" % (tag, err_x[:90]))

        # 18g) 库配对：C# 侧声明成 Rust 的库 => rc=2（两侧库不同名，拿错就是假差异）
        reset_place_state()
        s_p7 = execute(base_cfg(os.path.join(tmp, "run-place-pair"),
                                place_prof(chars_db={"csharp": "mir2_test_db_r"}),
                                scenario=scen_place))
        err_p7 = (s_p7["sides"].get("csharp") or {}).get("error") or ""
        chk(s_p7["exit_code"] == 2 and "不配对" in err_p7,
            "PL-12 角色库与该侧部署不配对 => rc=2（%r）" % err_p7[:90])

        # 18h) 缺 characters_db => rc=2（不猜库名）
        reset_place_state()
        prof_p8 = place_prof()
        prof_p8["sides"]["csharp"]["db"] = {
            "command": [sys.executable, stub, "db", "csharp", log, "{db_dir}"],
            "databases": ["mir2_test_db"]}
        s_p8 = execute(base_cfg(os.path.join(tmp, "run-place-nodb"), prof_p8,
                                scenario=scen_place))
        err_p8 = (s_p8["sides"].get("csharp") or {}).get("error") or ""
        chk(s_p8["exit_code"] == 2 and "characters_db" in err_p8,
            "PL-13 缺 db.characters_db => rc=2 点名（%r）" % err_p8[:90])

        # 18i) 表结构缺列（HomeMap）=> rc=2（不改表结构，直接拒绝）
        reset_place_state()
        prof_p9 = place_prof()
        prof_p9["sides"]["csharp"]["env"] = {"STUB_PLACEMENT_NO_HOMECOL": "1"}
        s_p9 = execute(base_cfg(os.path.join(tmp, "run-place-nocol"), prof_p9,
                                scenario=scen_place))
        err_p9 = (s_p9["sides"].get("csharp") or {}).get("error") or ""
        chk(s_p9["exit_code"] == 2 and "HomeMap" in err_p9,
            "PL-14 characters 缺 HomeMap 列 => rc=2（%r）" % err_p9[:90])

        # 18j) scenario.placement 形状非法 => 加载期 ConfigError（CLI rc=2），一步都不执行
        for label, bad in (("非法图名", {"map": "bad-map!", "x": 1, "y": 1}),
                           ("x 非整数", {"map": "0122", "x": "10", "y": 1}),
                           ("缺 y", {"map": "0122", "x": 1}),
                           ("多余键", {"map": "0122", "x": 1, "y": 1, "z": 2})):
            reset_place_state()
            shape_err = ""
            try:
                execute(base_cfg(os.path.join(tmp, "run-place-shape"), place_prof(),
                                 scenario=place_scenario(bad, "shape")))
            except ConfigError as e:
                shape_err = str(e)
            chk("placement" in shape_err and not os.path.exists(log),
                "PL-15 placement 形状非法（%s）=> ConfigError 且一步都没执行（%r）"
                % (label, shape_err[:80]))

        # 18k) --dry-run：打印预置命令、做离线校验、不建目录/不连库
        reset_place_state()
        out_pdry = os.path.join(tmp, "run-place-dry")
        s_pdry = execute(base_cfg(out_pdry, place_prof(), scenario=scen_place,
                                  dry_run=True))
        dry_steps_p = [c["step"] for c in s_pdry["commands"]]
        chk(s_pdry["exit_code"] == 0 and not os.path.exists(out_pdry)
            and not os.path.exists(log),
            "PL-16 --dry-run => rc=0，不建目录、不执行任何命令")
        chk(dry_steps_p[:3] == ["restore", "stack", "probe"]
            or "placement.columns" not in dry_steps_p,
            "PL-16 --dry-run 的预置是预览（不真连库，%s）" % dry_steps_p[:6])
        chk((s_pdry["placement"].get("sides", {}).get("csharp") or {}).get("rc") == 0
            and bool((s_pdry["placement"].get("sides", {}).get("csharp") or {})
                     .get("offline_check")),
            "PL-16 --dry-run 仍做离线内容校验（图/坐标可走）")

        # 18l) 预置失败在 dry-run 里也 rc=2
        s_pdrybad = execute(base_cfg(os.path.join(tmp, "run-place-drybad"),
                                     place_prof(), dry_run=True,
                                     scenario=place_scenario(
                                         {"map": "NOPE", "x": 1, "y": 1}, "drybad")))
        chk(s_pdrybad["exit_code"] == 2
            and "placement.columns" not in [c["step"] for c in s_pdrybad["commands"]],
            "PL-17 --dry-run 预置不可行 => rc=2 且不预览后续场景命令")

        # 19) 本卡的 pid 对账：纯函数层 8 条 + 接线 3 条。
        #     **必须在 RED/GREEN 判定之前计入**（review B3：放在 `if fail_n: return 1`
        #     之后时，失败只打印、rc 仍为 0，牙在门禁层面不可能变红）。
        for cond, msg in _selftest_pid_binaries(tmp):
            chk(cond, msg)
        for cond, msg in _selftest_pid_wiring(
                tmp, base_cfg, lambda **kw: _selftest_profile(tmp, stub, log, **kw)):
            chk(cond, msg)

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
