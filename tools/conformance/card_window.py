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
  * **`--var NAME=VALUE`（可重复）原样透传**（卡 `openmir2-card-window-var-passthrough`）：
    两侧（csharp / rust）probe 命令行里都逐字带上，顺序 = 命令行顺序；`--dry-run` 打印的
    命令里同样可见。加载期 fail-closed：缺 `=` / 变量名为空 / 值为空串 / 同名不同值 =>
    rc=2 并点名（绝不静默丢掉一个值）。**不传时行为与产物逐字节不变**。它只覆盖场景已声明
    的变量（覆值通道，不声明新变量），用于 `dice_chain` 这种 `params.npc_actor` 硬编码、
    又不在 `required`（因此没有 discovery 值）的场景。
  * **参数发现 fail-closed**（场景声明 `required` 时自动启用，`--discover` 可强制）：每侧
    先跑一次 discovery（默认 `npc_click_auto`：点一次 NPC 收 `SM_MERCHANTSAY(643)`），从
    **本侧**帧推导 required，再用本侧的值拼 `--var` 跑真正的场景。任一侧推不出 ⇒ 该侧
    rc=2 并**点名缺哪个变量**；两侧 `kind` 不一致（`derived` vs `rule`）或非会话变量
    （坐标等）取值不同 ⇒ rc=2。**绝不拿另一侧的值、绝不用默认值/空值填空**；每侧结果落
    `<out>/<side>/discovered.json`，两侧汇总落 `<out>/discovered.json`。

profile：
  * 内置 `alt`（默认）= 2026-10-23 双栈窗口所用的备用端口剖面
    （E:/tmp/conformance 私有根 + mir2_test_* / mir2_test_*_r 测试库）。
  * 环境加固（卡 `openmir2-window-profile-env-hardening`，window-run-1 §1 三处阻塞固化，
    alt / csharp-default 及其派生 profile 一体生效）：
      - restore 用 **Git bash 绝对路径**（裸 `bash` 会被 CreateProcess 先解析到 System32 的
        WSL 启动器 stub，rc=1）；加载期校验拒裸 `bash` / System32 路径（本机探测可用
        `CARD_WINDOW_GIT_BASH` 覆盖，preflight 的 `bash` 步同判据）。
      - stack 走 `tools/window/start-stack-wmi.ps1`：WMI `Win32_Process.Create` 派生
        （断开孙进程继承 stdout 管道导致的 stack 步 EOF 死锁）+ 等 `-ReadyPorts`
        （**含网关**）全监听 + 暖 45s，缺端口非零退出。
      - `ready_ports` **不含网关端口**（17200/17600/7200）：wait_ready 的连接探针会占住
        GameGate user 会话槽，596ms 后 probe 的真实连接复用该槽被丢（no SM_LOGON）。
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

资产声明通道（assets，卡 `openmir2-window-gold-declaration-channel`）：
  * 场景**顶层**可声明 `"assets": {"Gold": 10000, "note": "…"}`，表示「本场景要求角色资产
    列达到目标值」。首例：upgrade_retrieve 需要 Gold>=10000，而 placement（MapName/CX/CY）
    与 inventory（characters_bagitem）两通道都不覆盖 characters.Gold ⇒ 金币前置无法声明。
  * **列白名单**（首版仅 `Gold`）：非白名单列 / 非整数 / 负数 / 超 int32 => 加载期 rc=2
    点名（fail-closed）。扩展白名单只允许「有符号整数、语义是资产/点数」的列（实表取证：
    characters.Gold 是 int(11)）；绝不加字符串列/时间列/主键外键列。
  * 每侧 restore 之后、起栈之前 UPDATE 该侧**测试库** characters 的这一行（按 profile
    char_name/account 定位，ChrName+LoginID 唯一、未删除）；写前后对**其余列**做 MD5
    指纹对比（只许写声明列），写后读回断言目标值；完成不了 rc=2，绝不让 probe 超时。
  * `--dry-run` 打印预览行（含 `Gold=10000`）并做离线校验（库名/库配对），不连库。
  * 边界：`tools/probe/mir2_probe.py` 的场景顶层键白名单暂未放行 `assets`——场景 JSON
    采用本通道时需在采卡 delta 里给 probe 加一行 optional 键（形状校验与本工具
    `scenario_assets` 同口径）。本卡不碰场景 JSON、不碰 probe。

用法：
  CONFORMANCE_PASSWORD=... python -I -B tools/conformance/card_window.py \
      --card openmir2-window-529-dualrun \
      --scenario tools/probe/scenarios/login_only.json \
      --out E:/tmp/conformance/runs/529-card-window \
      --profile alt

  # 只打印将要执行的完整命令序列（不起栈、不建目录、不需要密码）
  python -I -B tools/conformance/card_window.py --card X --scenario S --out O --dry-run

  # 操作者透传 --var（可重复）：原样进两侧 probe 命令行；非法形式加载期 rc=2
  python -I -B tools/conformance/card_window.py --card X \
      --scenario tools/probe/scenarios/dice_chain.json --out O \
      --var npc_actor=<本次 actor id> --dry-run

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
import contextlib
import copy
import hashlib
import io
import json
import os
import re
import shutil
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

# --- restore 的 bash 必须写 Git bash 绝对路径（卡 openmir2-window-profile-env-hardening）---
# CreateProcess 对裸 `bash` 的搜索顺序是「应用目录 → 当前目录 → **System32** → Windows 目录
# → PATH」，System32 先于 PATH：机器开着 WSL 功能时 `C:\Windows\System32\bash.exe`（WSL 启动器，
# 未装发行版 rc=1）会吃掉裸 `bash`，restore 必败（window-run-1 §1.1 实测）。所以 restore 的
# 可执行位**不许**是裸 `bash`，也**不许**直接指向 System32/Sysnative 的 stub。
def is_wsl_stub_path(token):
    """bash 可执行路径落在 System32/Sysnative 下 ⇒ WSL 启动器 stub（未装发行版 rc=1）。"""
    p = os.path.normcase(str(token).replace("/", os.sep))
    return (os.path.basename(p).startswith("bash")
            and ("system32" in os.path.dirname(p) or "sysnative" in os.path.dirname(p)))


GIT_BASH_CANDIDATES = (
    r"C:\Program Files\Git\usr\bin\bash.exe",
    r"C:\Program Files (x86)\Git\usr\bin\bash.exe",
    os.path.expandvars(r"%LocalAppData%\Programs\Git\usr\bin\bash.exe"),
)


def detect_git_bash():
    """探测本机 Git bash 绝对路径：环境变量 `CARD_WINDOW_GIT_BASH` → 常见安装位 → PATH
    （PATH 命中也跳过 WSL stub）。找不到返回 None——内置 profile 会回退裸 `bash`，
    加载期由 check_restore_bash fail-closed 拒绝并给出修法。"""
    override = os.environ.get("CARD_WINDOW_GIT_BASH", "").strip()
    for c in ([override] if override else []) + list(GIT_BASH_CANDIDATES):
        if c and os.path.isfile(c) and not is_wsl_stub_path(c):
            return c.replace(os.sep, "/")
    found = shutil.which("bash")
    if found and os.path.isfile(found) and not is_wsl_stub_path(found):
        return found.replace(os.sep, "/")
    return None


GIT_BASH = detect_git_bash()


def check_restore_bash(argv, where):
    """restore 可执行位的加载期校验（Windows）：拒裸 `bash` 与 System32/Sysnative stub。"""
    if os.name != "nt" or not argv:
        return argv
    exe = str(argv[0])
    if is_wsl_stub_path(exe):
        raise ConfigError(
            "%s 的 bash 指向 System32/Sysnative（WSL 启动器 stub，未装发行版 rc=1）：%r"
            " ⇒ 改写 Git bash 绝对路径（如 C:/Program Files/Git/usr/bin/bash.exe），"
            "或安装 Git for Windows / 设 CARD_WINDOW_GIT_BASH" % (where, exe))
    if os.path.basename(exe).lower() in ("bash", "bash.exe") and not os.path.dirname(exe):
        hint = ("（本机探测到 %s）" % GIT_BASH) if GIT_BASH else (
            "（本机未探测到 Git bash：装 Git for Windows 或设 CARD_WINDOW_GIT_BASH）")
        raise ConfigError(
            "%s 用裸 `bash`：CreateProcess 在 PATH 之前先查 System32，会命中 WSL 启动器"
            " stub、restore 必败（window-run-1 §1.1 实测）⇒ 改写 Git bash 绝对路径%s"
            % (where, hint))
    return argv


# --- 内置栈命令走 WMI Win32_Process.Create 派生 + 全端口监听兜底 + 暖机（同卡）---
# (1) `Start-Process -RedirectStandardOutput` 派生的服务端**孙进程**继承 python↔powershell 的
#     stdout 管道写端；stack 步 `subprocess.run(timeout=600)` 超时 kill 后 `communicate()`
#     等 EOF 永不到来 ⇒ 死锁（window-run-1 §1.2，pipe_repro 最小复现）。WMI 派生的父进程是
#     WmiPrvSE，无句柄继承 ⇒ stack 步能返回。
# (2) GameGate 探针占 user 会话槽（§1.3）⇒ 内置 profile 的 `ready_ports` **不含网关端口**
#     （17200/17600/7200）；网关就绪由包装脚本兜底：等 `-ReadyPorts`（**含**网关）全部监听
#     + 暖 45s，缺端口非零退出（fail-closed）。
WMI_STACK_WRAPPER = os.path.join(
    REPO_ROOT, "tools", "window", "start-stack-wmi.ps1").replace(os.sep, "/")


def wmi_stack_argv(inner_script, ready_ports, pin_checks=None, pin_log=None):
    """内置 profile 的 stack 命令：WMI 派生 + 就绪端口（**含网关**）+ 暖机，fail-closed。

    卡 `openmir2-window-wmi-env-forwarding`：pin_checks/pin_log 给出时追 `-PinChecks`/
    `-PinLog`——起栈后由包装校验 server 日志自证行（如 game-svr 的
    `CLIENT_ABILITY_LAYOUT layout=oracle-40（MIR2_CLIENT_ABILITY_LAYOUT=0）`），
    pin 没到/布局不符 ⇒ 包装非零退出（fail-closed）。"""
    argv = POWERSHELL + ["-File", WMI_STACK_WRAPPER,
                         "-Script", str(inner_script),
                         "-ReadyPorts", ",".join(str(p) for p in ready_ports)]
    if pin_checks:
        argv += ["-PinChecks", ",".join(str(x) for x in pin_checks)]
    if pin_log:
        argv += ["-PinLog", str(pin_log)]
    return argv


# --- pin 可观测（卡 `openmir2-window-wmi-env-forwarding`）------------------
# WMI `Win32_Process.Create` 的父进程是 WmiPrvSE，不继承调用方环境：python 调用面
# 设好的 `sides.<side>.env` pin 到 server 就丢（53 分诊实锤：game-svr.out.log
# 05:57:06.620 `layout=client-58（MIR2_CLIENT_ABILITY_LAYOUT=<unset>）`——静默跑错布局）。
# 修法在包装（tools/window/start-stack-wmi.ps1）：Win32_ProcessStartup 全量转发
# 调用方 env + 起栈后按 -PinChecks/-PinLog 校验 server 日志自证行。
PIN_LAYOUT_KEY = "MIR2_CLIENT_ABILITY_LAYOUT"
WMI_WRAPPER_TOKEN = "start-stack-wmi.ps1"
# rust 侧 game-svr 自证日志（部署脚本 -LogDir 写死这里，口径同 alt 的 pid_file）。
RUST_STACK_PIN_LOG = "E:/tmp/conformance/rust-logs/game-svr.out.log"


def is_wmi_stack_argv(argv):
    """stack 命令是否走 WMI 包装（只有它能接 pin 自证）。"""
    return any(WMI_WRAPPER_TOKEN in str(a) for a in (argv or []))


def wmi_pin_checks(stack_argv):
    """已接线的 -PinChecks 列表；WMI 包装未接线 ⇒ []，非 WMI 链 ⇒ None。"""
    if not is_wmi_stack_argv(stack_argv):
        return None
    argv = [str(a) for a in (stack_argv or [])]
    if "-PinChecks" in argv:
        i = argv.index("-PinChecks")
        if i + 1 < len(argv):
            return [x for x in argv[i + 1].split(",") if x]
        return []
    return []


def with_wmi_pin_checks(stack_argv, pin_checks, pin_log):
    """在既有 WMI 包装 argv 上接线/替换 -PinChecks/-PinLog（幂等）。非 WMI 链 ⇒ ConfigError。"""
    if not is_wmi_stack_argv(stack_argv):
        raise ConfigError(
            "只有 %s 的 stack 命令能接 pin 自证（实际 %r）——非 WMI 链的 env 转发由"
            "调用方进程保证，无需（也无法）经包装校验" % (WMI_WRAPPER_TOKEN, stack_argv))
    argv = [str(a) for a in (stack_argv or [])]
    out, skip = [], False
    for tok in argv:
        if skip:
            skip = False
            continue
        if tok in ("-PinChecks", "-PinLog"):
            skip = True
            continue
        out.append(tok)
    return out + ["-PinChecks", ",".join(str(x) for x in pin_checks),
                  "-PinLog", str(pin_log)]


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
                        "测试库 mir2_test_* / mir2_test_*_r，同一组 15xxx/17xxx 端口，两栈串行。"
                        " | 环境加固（卡 openmir2-window-profile-env-hardening，window-run-1 §1）："
                        "restore 用 Git bash 绝对路径（裸 bash 会被 CreateProcess 先解析到 "
                        "System32 的 WSL stub）；stack 走 tools/window/start-stack-wmi.ps1 "
                        "（WMI Win32_Process.Create 派生，断开 stdout 管道继承的死锁）并兜底"
                        "「全就绪端口（含网关）监听 + 暖 45s」；ready_ports 去掉网关端口 "
                        "17200/17600（wait_ready 的探针会占住 GameGate user 会话槽，"
                        "596ms 后真实连接被丢 no SM_LOGON）。"),
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
                # 卡 openmir2-window-csharp-rng：C# 栈改用**仓库内**启动器（E:/tmp 会被清扫）。
                # 它在 `--rng-log` 时给 GameSvr 注入 DOTNET_STARTUP_HOOKS/MIR2_RNG_SEED/
                # MIR2_RNG_LOG/MIR2_RNG_SITE（影子 M2GameSvr-hook，冻结基线不动）。
                "stack": wmi_stack_argv(
                    os.path.join(REPO_ROOT, "tools", "window", "start-csharp.ps1").replace(os.sep, "/"),
                    (15000, 17000, 17100, 17200)),
                "stop": CSHARP_STOP,
                "restore": [GIT_BASH or "bash",
                            "E:/tmp/conformance/restore-baseline.sh"],
                "ports": [15000, 15100, 15500, 15600, 15700, 16000, 17000, 17100,
                          17200, 17201, 17202],
                # 网关 17200 不进 ready_ports（探针占会话槽）；由 stack 包装的
                # -ReadyPorts 15000,17000,17100,17200 兜底等齐 + 暖机。
                "ready_ports": [15000, 17000, 17100],
                "probe_extra": [],
                "binaries": ["E:/tmp/conformance/csharp-root/M2GameSvr/GameSrv.exe"],
                "envir_root": "E:/tmp/conformance/csharp-root/M2GameSvr",
                "db": {"preset": "test", "characters_db": "mir2_test_db"},
            },
            "rust": {
                "label": "Rust",
                "stack": wmi_stack_argv("E:/tmp/conformance/start-rust.ps1",
                                        (15000, 17000, 17100, 17600)),
                "stop": POWERSHELL + ["-File", "E:/tmp/conformance/stop-rust.ps1"],
                "restore": [GIT_BASH or "bash",
                            "E:/tmp/conformance/restore-baseline-r.sh"],
                "ports": [15000, 15100, 15500, 15600, 15700, 16000, 17000, 17100, 17600],
                # 网关 17600 同上：不进 ready_ports，由 stack 包装兜底。
                "ready_ports": [15000, 17000, 17100],
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
                        "默认拒绝运行（内容冻结，窗口只跑测试库）。"
                        " | 环境加固与 alt 同批（卡 openmir2-window-profile-env-hardening）："
                        "restore 用 Git bash 绝对路径；stack 走 WMI 包装；ready_ports 去掉"
                        "网关 7200（由 start-stack-wmi.ps1 的 -ReadyPorts 兜底 + 暖机）。"),
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
                "stack": wmi_stack_argv("E:/MirServer/start-all.ps1",
                                        (5000, 7000, 7100, 7200)),
                "stop": POWERSHELL + ["-File", "E:/MirServer/start-all.ps1", "-Stop"],
                "restore": [GIT_BASH or "bash",
                            "E:/tmp/conformance/restore-baseline.sh"],
                "ports": [5000, 5100, 5500, 5600, 5700, 6000, 7000, 7100, 7200],
                # GameGate 7200 不进 ready_ports（同 alt 的 17200 口径）。
                "ready_ports": [5000, 7000, 7100],
                "probe_extra": [],
                "binaries": ["E:/MirServer/M2GameSvr/GameSrv.exe"],
                "db": {"preset": "prod", "characters_db": "mir2_db"},
            },
            "rust": {
                "label": "Rust（默认端口，需部署在 E:/tmp/conformance/rust-default-root）",
                "stack": wmi_stack_argv("E:/tmp/conformance/start-rust-default.ps1",
                                        (5000, 7000, 7100, 7200)),
                "stop": POWERSHELL + ["-File", "E:/tmp/conformance/stop-rust-default.ps1"],
                "restore": [GIT_BASH or "bash",
                            "E:/tmp/conformance/restore-baseline.sh"],
                "ports": [5000, 5100, 5500, 5600, 5700, 6000, 7000, 7100, 7200],
                "ready_ports": [5000, 7000, 7100],
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


def _derived_magic_profile(base, name, description):
    """构造「alt + 两侧 magic + D-9 pin」的内置 profile（卡 `openmir2-window-profile-magic-preset`）。

    只加数据、不加判定逻辑：``magic`` 走既有 profile 级夹具通道（README §1.8），
    ``MIR2_CLIENT_ABILITY_LAYOUT=0`` 走 ``sides.rust.env`` 通道（README「spell_cast 必须 pin D-9」；
    C# 是 oracle 不消费该 env，卡 `openmir2-window-wmi-env-forwarding` 起不再两侧都设），
    并给 rust 侧 WMI stack 接线 -PinChecks（pin 自证，见 with_wmi_pin_checks）。
    **不改 `alt` 默认值**：alt 是 18 个场景的既有基线，动它会让所有场景的输入一起变；
    新前置只能以**新 profile** 的形式存在。
    """
    prof = copy.deepcopy(base)
    prof["name"] = name
    prof["description"] = description
    prof["magic"] = [{"magic_id": 1, "level": 0, "use_key": "1", "curr_train": 0}]
    # 卡 `openmir2-window-wmi-env-forwarding` 收紧：布局 pin 只设 **rust** 侧——
    # `MIR2_CLIENT_ABILITY_LAYOUT` 的消费方只有 rust（C# 是 oracle，读都不读该 env；
    # 旧写法两侧都设是噪音，且让「声明 pin ⇒ 必须可观测」的加载期守卫无处安放）。
    rs = prof["sides"].get("rust")
    if rs is not None:
        env = dict(rs.get("env") or {})
        env[PIN_LAYOUT_KEY] = "0"
        rs["env"] = env
        # pin 声明了就要可观测：rust 侧 stack 是 WMI 包装 ⇒ 接线 -PinChecks/-PinLog
        #（起栈后由包装校验 game-svr 日志自证行，不一致非零退出）。
        if is_wmi_stack_argv(rs.get("stack")):
            rs["stack"] = with_wmi_pin_checks(
                rs["stack"], ["%s=%s" % (PIN_LAYOUT_KEY, env[PIN_LAYOUT_KEY])],
                RUST_STACK_PIN_LOG)
    return prof


BUILTIN_PROFILES["window-magic"] = _derived_magic_profile(
    BUILTIN_PROFILES["alt"],
    "window-magic",
    ("alt 的副本 + `magic` 夹具（magic_id=1）+ `sides.rust.env."
     "MIR2_CLIENT_ABILITY_LAYOUT=0`：让 `spell_cast` 无需临时手工 profile 即可 dry-run 通过"
     "（卡 `openmir2-window-profile-magic-preset`）。"
     " | pin 可观测（卡 `openmir2-window-wmi-env-forwarding`）：rust 侧 stack 接线"
     " `-PinChecks MIR2_CLIENT_ABILITY_LAYOUT=0 -PinLog`（game-svr 日志自证行，"
     "不一致包装非零退出）。"),
)


def _derived_testdb_profile(base, name, description):
    """构造「alt + C# 侧显式指向测试库」的内置 profile（卡 `openmir2-window-profile-testdb-preset`）。

    只加数据、不加判定逻辑：C# 侧 ``db.preset=test`` / ``db.characters_db=mir2_test_db``
    走既有 profile 通道，Rust 侧沿用 alt 的 ``mir2_test_db_r``。
    **不改 `alt` 默认值**：alt 是 18 个场景的既有基线，动它会让所有场景的输入一起变。
    **不削弱生产库守卫**：``check_profile_safety`` 的 ``prod(...)`` 检查一字不动；
    本 profile 只是让输入展开后的库名落到 test，从而不再命中守卫。
    """
    prof = copy.deepcopy(base)
    prof["name"] = name
    prof["description"] = description
    cs = prof["sides"]["csharp"]
    db = dict(cs.get("db") or {})
    db["preset"] = "test"
    db["characters_db"] = "mir2_test_db"
    cs["db"] = db
    return prof


BUILTIN_PROFILES["window-testdb"] = _derived_testdb_profile(
    BUILTIN_PROFILES["alt"],
    "window-testdb",
    ("alt 的副本 + C# 侧显式指向测试库（`db.preset=test` / "
     "`db.characters_db=mir2_test_db`，Rust 侧沿用 `mir2_test_db_r`）。"
     "让 `upgrade_retrieve` / `equip_struck_save` / `bag_equip` / `eat_item` / "
     "`npc_script_chain` 这 5 个带预置位的场景在开窗前即可 dry-run 通过"
     "（卡 `openmir2-window-profile-testdb-preset`）。"
     "**仅指向测试库；真跑会写测试库，绝不可用于生产。**"),
)

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
        # restore 的 bash 必须 Git bash 绝对路径（裸 bash / System32 stub 加载期拒绝；
        # 卡 openmir2-window-profile-env-hardening——内置 profile 也受同一校验管）。
        check_restore_bash(sc["restore"], "profile.sides.%s.restore" % side)
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
        # 卡 `openmir2-window-wmi-env-forwarding`：声明了布局 pin，就必须可观测——
        # stack 走 WMI 包装时必须接 -PinChecks（起栈后校验 server 日志自证行，
        # 不一致 fail-closed）；否则 pin 丢失是静默的（53 分诊的实锤缺口）。
        pin_val = (sc.get("env") or {}).get(PIN_LAYOUT_KEY)
        if pin_val is not None:
            wired = wmi_pin_checks(sc.get("stack"))
            want = "%s=%s" % (PIN_LAYOUT_KEY, pin_val)
            if wired is not None and want not in wired:
                raise ConfigError(
                    "profile.sides.%s.env.%s=%r 声明了 pin，但 stack 是 WMI 包装且未接 "
                    "-PinChecks ⇒ pin 到没到不可观测（静默跑错布局缺口）⇒ 用 "
                    "wmi_stack_argv(..., pin_checks=['%s'], pin_log=...) 或 "
                    "with_wmi_pin_checks() 接线；非 WMI 链不受此限" % (side, PIN_LAYOUT_KEY, pin_val, want))
        er = sc.get("envir_root")
        if er is not None and (not isinstance(er, str) or not er):
            raise ConfigError("profile.sides.%s.envir_root 必须是非空字符串（部署内容根目录）"
                              % side)
    validate_db_client(prof.get("db_client"), "profile.db_client")
    for side in SIDES:
        validate_db_client(sides[side].get("db_client"),
                           "profile.sides.%s.db_client" % side)
    # 魔法夹具声明（卡 `openmir2-window-magic-fixture`）：加载期 fail-closed。
    fixtures_by_side = {}
    for side in SIDES:
        profile_magic(prof, side)
        # 卡 `openmir2-window-fixture-castle`：窗口夹具（怪物 + 配置覆盖 + 城堡前置）加载期
        # fail-closed（未知字段/键、越界值、坏城名）；`castle_list` 还要两侧一致（review F-1）。
        fixtures_by_side[side] = profile_fixtures(prof, side)
    _check_fixture_symmetry(fixtures_by_side)
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


def parse_var_args(items):
    """`--var NAME=VALUE`（可重复）=> probe argv 片段；**加载期** fail-closed。

    卡 `openmir2-card-window-var-passthrough`：窗口编排此前没有把操作者输入喂给 probe 的通道，
    `dice_chain` 这类 `params.npc_actor` 硬编码场景只能点到旧会话的 actor。
      * 原样保留 `NAME=VALUE`（只按**第一个** `=` 切，值里允许再有 `=`；与 mir2_probe 的
        `parse_vars` 同口径），顺序 = 命令行顺序（确定性，两侧一致）；
      * 缺 `=` / 变量名为空 / 值为空串 / **同名不同值** => ConfigError（rc=2 并点名），
        绝不静默丢弃其中一个值；
      * 完全相同的重复项只保留一次（值不变，透传结果相同）。
    """
    out = []
    seen = {}
    for idx, item in enumerate(items or [], 1):
        if not isinstance(item, str):
            raise ConfigError("--var 第 %d 个必须是字符串 NAME=VALUE（实际 %r）"
                              % (idx, item))
        if "=" not in item:
            raise ConfigError("--var 第 %d 个缺少 '='（必须是 NAME=VALUE）：%r"
                              % (idx, item))
        name, _, value = item.partition("=")
        if not name:
            raise ConfigError("--var 第 %d 个的变量名为空：%r" % (idx, item))
        if value == "":
            raise ConfigError("--var %s 的值为空字符串：拒绝静默当默认值（%r）"
                              % (name, item))
        if name in seen:
            if seen[name] != value:
                raise ConfigError("--var %s 重复且值不同（%r vs %r）：拒绝静默丢掉其中一个"
                                  % (name, seen[name], value))
            continue
        seen[name] = value
        out += ["--var", "%s=%s" % (name, value)]
    return out


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


BLOCKED_REQUIRED = ("reason",)
BLOCKED_OPTIONAL = ("missing_fixture",)


def scenario_blocked(path):
    """读场景顶层的 `blocked` 声明；没有 => None；形状非法 => ConfigError(fail-closed)。

    场景可以声明自己被**封存**：形状合法（probe 的 `--check-scenario` 仍 rc=0），但**不可跑**——
    「为什么跑不了 / 缺什么夹具」写在顶层机器可读字段里，而不是写在 description 人话里
    （窗口批次读不到人话）。本工具见到 `blocked` 一律 rc=2，且**不建 out、不起栈、不写 report**，
    从产物层杜绝「跑了一份 BLOCKED 场景」。

    schema（与 `tools/probe/mir2_probe.py::_check_blocked` 同口径，两处各自 fail-closed）：::

        "blocked": {"reason": "<非空 str>", "missing_fixture": "<str，可选>"}

    与 required/computed/placement 一样**重读场景文件**：不依赖 probe 那一侧是否跑过。
    """
    try:
        with open(path, "r", encoding="utf-8-sig") as f:
            data = json.load(f)
    except (OSError, ValueError) as e:
        raise ConfigError("%s: cannot load scenario: %s" % (path, e))
    if not isinstance(data, dict):
        raise ConfigError("%s: scenario must be a JSON object" % path)
    bl = data.get("blocked")
    if bl is None:
        return None
    if not isinstance(bl, dict):
        raise ConfigError("%s: scenario.blocked 必须是对象（reason[/missing_fixture]）" % path)
    bad_keys = set(bl) - set(BLOCKED_REQUIRED) - set(BLOCKED_OPTIONAL)
    if bad_keys:
        raise ConfigError("%s: scenario.blocked 有未知键 %s（fail-closed）"
                          % (path, sorted(bad_keys)))
    reason = bl.get("reason")
    if not isinstance(reason, str) or not reason.strip():
        raise ConfigError("%s: scenario.blocked.reason 必须是非空字符串（实际 %r）："
                          "封存理由必须机器可读，不能只写在 description 里" % (path, reason))
    mf = bl.get("missing_fixture")
    if mf is not None and (not isinstance(mf, str) or not mf.strip()):
        raise ConfigError("%s: scenario.blocked.missing_fixture 给了就必须是非空字符串"
                          "（实际 %r）" % (path, mf))
    return {"reason": reason, "missing_fixture": mf or ""}


# ---------------------------------------------------------------------------
# 夹具前置声明（`requires`）：场景显式声明「跑我需要的 DB 夹具」
#
# 卡 `openmir2-scenario-fixture-precondition`：`spell_cast` 摘掉 `blocked` 后暴露的假绿通道——
# 场景需要 `characters_magic` 夹具，但 profile 没有 `magic` 段时 `profile_magic` 返回 None，
# dry-run 的 magic 段整段跳过 ⇒ **rc=0 却根本没预置**，真跑才在 `wait_for` 上超时。
# 现在：场景顶层**显式声明** `requires`，本工具**加载期**按**生效 profile**（含 `sides.<side>`
# 覆盖）逐侧核段；缺段 ⇒ rc=2 并点名。位置与 BLOCKED 硬门同级：`--dry-run` 与真跑走同一条路径，
# 免得 dry-run 绿被误读成「可以真跑」。
#
# **只认 `magic`**（别的先不实现）：未知值 fail-closed 并点名，避免「写了值却没生效」再次假绿。
# ---------------------------------------------------------------------------
REQUIRES_KNOWN = ("magic",)


def scenario_requires(path):
    """读场景顶层 `requires`；没有 => `{"name": ..., "requires": []}`。

    形状/取值非法一律 ConfigError（fail-closed，rc=2）：
      * `requires` 必须是非空、无重复的非空字符串数组；
      * 只认 `REQUIRES_KNOWN`，未知值点名（拼错/未实现的值不许被静默忽略）。
    """
    try:
        with open(path, "r", encoding="utf-8-sig") as f:
            data = json.load(f)
    except (OSError, ValueError) as e:
        raise ConfigError("%s: cannot load scenario: %s" % (path, e))
    if not isinstance(data, dict):
        raise ConfigError("%s: scenario must be a JSON object" % path)
    name = data.get("name")
    if not isinstance(name, str) or not name:
        name = os.path.splitext(os.path.basename(path))[0]
    rq = data.get("requires")
    if rq is None:
        return {"name": name, "requires": []}
    if (not isinstance(rq, list) or not rq
            or any(not isinstance(x, str) or not x for x in rq)):
        raise ConfigError("%s: scenario.requires 必须是非空字符串数组（实际 %r）"
                          % (path, rq))
    dup = sorted({x for x in rq if rq.count(x) > 1})
    if dup:
        raise ConfigError("%s: scenario.requires 有重复值 %s（fail-closed）" % (path, dup))
    bad_values = [x for x in rq if x not in REQUIRES_KNOWN]
    if bad_values:
        raise ConfigError("%s: scenario.requires 含未知夹具 %s（只认 %s；fail-closed）"
                          % (path, bad_values, list(REQUIRES_KNOWN)))
    return {"name": name, "requires": list(rq)}


def validate_scenario_requires(scenario, decl, prof, profile_label):
    """场景声明的夹具必须由**生效 profile** 提供，否则 ConfigError（rc=2）。

    `magic` 要在**两侧**分别写各侧测试库，所以逐侧判：`profile_magic(prof, side)` 任一侧为
    None 即红（顶层 `magic` 应用到两侧；`sides.<side>.magic` 可覆盖本侧）。
    """
    for kind in decl.get("requires") or []:
        if kind != "magic":       # 未知值已在 scenario_requires 里 fail-closed
            continue
        missing = [s for s in SIDES if not profile_magic(prof, s)]
        if missing:
            raise ConfigError(
                "场景 `%s` 要求 `magic` 夹具，但 profile `%s` 未声明 `magic` 段"
                "（或某侧被覆盖成空）：缺 %s 侧（场景文件 %s）"
                % (decl.get("name") or os.path.basename(scenario), profile_label,
                   "、".join(missing), scenario))


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

# ---------------------------------------------------------------------------
# 魔法夹具（magic）：restore 之后、起栈之前把「角色必须会的魔法」写进该侧测试库
# `characters_magic`（卡 `openmir2-window-magic-fixture`）。
#
# 为什么是 DB 预置：`spell_cast` 场景的 `blocked.missing_fixture` 指出缺这一行 ⇒
# `GetMagicInfo` 为空 ⇒ 施法只回 `SM_MOVEFAIL(28)`，**永远等不到 638/639**。
# 技能不是脚本动作、GM 权限恒 0 ⇒ 与预置位/背包同一选型：只在 restore 之后、
# 起栈之前改**测试库**。
#
# 声明形态（profile 顶层，应用到两侧；`sides.<side>.magic` 可覆盖本侧）：
#   "magic": [{"magic_id": 1, "level": 0, "use_key": "1", "curr_train": 0}]
# - 四列必填；`magic_id` 1..65535、`level` 0..3、`use_key` 单个 ASCII 可见字符、
#   `curr_train` >= 0；未知键/类型错/越界 => 加载期 ConfigError（rc=2）。
# - **跨栈不同的字段不进判据**：`PlayerId`（= 本侧 `characters.Id`）与行 `Id` 各侧自己定位，
#   读回只比 `MagicId/Level/UseKey/CurrTrain`。
# ---------------------------------------------------------------------------
MAGIC_REQUIRED_KEYS = ("magic_id", "level", "use_key", "curr_train")
MAGIC_OPTIONAL_KEYS = ("note",)
MAGIC_ID_MAX = 0xFFFF
MAGIC_LEVEL_MAX = 3


def _validate_magic_list(magic, where):
    """形状+schema 校验（fail-closed）：非法即 ConfigError。"""
    if not isinstance(magic, list) or not magic:
        raise ConfigError("%s 必须是非空数组" % where)
    out = []
    seen = set()
    for i, item in enumerate(magic):
        w = "%s[%d]" % (where, i)
        if not isinstance(item, dict):
            raise ConfigError("%s 必须是对象" % w)
        bad = set(item) - set(MAGIC_REQUIRED_KEYS) - set(MAGIC_OPTIONAL_KEYS)
        if bad:
            raise ConfigError("%s 有未知键 %s（fail-closed）" % (w, sorted(bad)))
        missing = [k for k in MAGIC_REQUIRED_KEYS if k not in item]
        if missing:
            raise ConfigError("%s 缺必填键 %s" % (w, missing))
        mid = item["magic_id"]
        if not isinstance(mid, int) or isinstance(mid, bool):
            raise ConfigError("%s.magic_id 必须是整数（实际 %r）" % (w, mid))
        if mid < 1 or mid > MAGIC_ID_MAX:
            raise ConfigError("%s.magic_id=%r 越界（1..%d）" % (w, mid, MAGIC_ID_MAX))
        lv = item["level"]
        if not isinstance(lv, int) or isinstance(lv, bool):
            raise ConfigError("%s.level 必须是整数（实际 %r）" % (w, lv))
        if lv < 0 or lv > MAGIC_LEVEL_MAX:
            raise ConfigError("%s.level=%r 越界（0..%d）" % (w, lv, MAGIC_LEVEL_MAX))
        key = item["use_key"]
        if (not isinstance(key, str) or len(key) != 1
                or not (0x21 <= ord(key) <= 0x7E)):
            raise ConfigError("%s.use_key 必须是单个 ASCII 可见字符（实际 %r）" % (w, key))
        tr = item["curr_train"]
        if not isinstance(tr, int) or isinstance(tr, bool):
            raise ConfigError("%s.curr_train 必须是整数（实际 %r）" % (w, tr))
        if tr < 0:
            raise ConfigError("%s.curr_train=%r 必须 >= 0" % (w, tr))
        if mid in seen:
            raise ConfigError("%s.magic_id=%d 与前面的条目重复（同一魔法只能声明一次）"
                              % (w, mid))
        seen.add(mid)
        if item.get("note") is not None and not isinstance(item["note"], str):
            raise ConfigError("%s.note 必须是字符串" % w)
        out.append({"magic_id": mid, "level": lv, "use_key": key, "curr_train": tr,
                    "note": item.get("note") or ""})
    return out


def profile_magic(prof, side=None):
    """读 profile 的魔法声明：顶层 `magic` 应用于两侧；`sides.<side>.magic` 覆盖本侧。

    没有声明 => None；形状非法 => ConfigError（加载期 fail-closed）。
    """
    sides = prof.get("sides") or {}
    if side is not None:
        sc = sides.get(side) or {}
        if "magic" in sc:
            return _validate_magic_list(sc["magic"], "profile.sides.%s.magic" % side)
    if "magic" not in prof:
        return None
    return _validate_magic_list(prof["magic"], "profile.magic")


def _magic_char_sql(db, chr_name, login_id, has_deleted):
    tail = ", `Deleted`" if has_deleted else ""
    return ("/* magic */ SELECT COUNT(*), COALESCE(MIN(`Id`), 0)%s FROM `%s`.`characters` "
            "WHERE `ChrName` = %s AND `LoginID` = %s;\n"
            % (tail, db, _sql_quote(chr_name), _sql_quote(login_id)))


def _magic_dup_sql(db, player_id, ids):
    return ("/* magic */ SELECT `MagicId`, COUNT(*) FROM `%s`.`characters_magic` "
            "WHERE `PlayerId` = %d AND `MagicId` IN (%s) GROUP BY `MagicId` "
            "HAVING COUNT(*) > 1;\n"
            % (db, int(player_id), ", ".join(str(int(x)) for x in ids)))


def _magic_rows_sql(db, player_id, ids):
    return ("/* magic */ SELECT `MagicId`, `Level`, `UseKey`, `CurrTrain` FROM `%s`.`characters_magic` "
            "WHERE `PlayerId` = %d AND `MagicId` IN (%s) ORDER BY `MagicId`;\n"
            % (db, int(player_id), ", ".join(str(int(x)) for x in ids)))


def _magic_fingerprint_sql(db, player_id, ids):
    return ("/* magic */ SELECT COUNT(*), COALESCE(MIN(MD5(CONCAT_WS(CHAR(31), `MagicId`, `Level`, "
            "`UseKey`, `CurrTrain`))), '') FROM `%s`.`characters_magic` "
            "WHERE `PlayerId` = %d AND `MagicId` NOT IN (%s);\n"
            % (db, int(player_id), ", ".join(str(int(x)) for x in ids)))


def _magic_write_sql(db, player_id, m, exists):
    if exists:
        return ("/* magic */ UPDATE `%s`.`characters_magic` SET `Level` = %d, `UseKey` = %s, "
                "`CurrTrain` = %d WHERE `PlayerId` = %d AND `MagicId` = %d;\n"
                % (db, int(m["level"]), _sql_quote(m["use_key"]), int(m["curr_train"]),
                   int(player_id), int(m["magic_id"])))
    return ("/* magic */ INSERT INTO `%s`.`characters_magic` (`PlayerId`, `MagicId`, `Level`, "
            "`UseKey`, `CurrTrain`) VALUES (%d, %d, %d, %s, %d);\n"
            % (db, int(player_id), int(m["magic_id"]), int(m["level"]),
               _sql_quote(m["use_key"]), int(m["curr_train"])))


def _parse_magic_rows(out):
    """解析 `MagicId/Level/UseKey/CurrTrain` 的 SELECT 输出（mysql --batch TSV）。"""
    rows = {}
    for ln in (out or "").splitlines():
        parts = ln.split("\t")
        if len(parts) < 4 or not parts[0].strip():
            continue
        try:
            rows[int(parts[0])] = {"level": int(parts[1]), "use_key": parts[2],
                                   "curr_train": int(parts[3])}
        except ValueError:
            raise ConfigError("characters_magic 返回无法解析：%r" % (ln,))
    return rows


def _magic_snapshot(magic, rows):
    """证据/判据用的只读快照：只含声明字段（不含 PlayerId/Id 这类跨栈不同值）。"""
    out = []
    for m in magic:
        found = rows.get(m["magic_id"])
        out.append({"magic_id": m["magic_id"],
                    "declared": {k: m[k] for k in ("level", "use_key", "curr_train")},
                    "found": found})
    return out


def run_magic(runner, side, prof, sc, magic, env, timeout=120):
    """在一侧预置魔法：restore 之后、起栈之前。返回证据 dict（rc=0 才算完成）。

    fail-closed：角色不存在/多行/已删除、同一 MagicId 多行、写后读回不符、其它魔法行
    指纹变化 ⇒ rc=2 并点名，**绝不让 probe 去超时**。
    """
    res = {
        "side": side,
        "source": "profile",
        "declared": [dict(m) for m in magic],
        "db": None,
        "db_pairing": None,
        "player_id": None,
        "before": None,
        "after": None,
        "action": None,
        "sql": {"character": None, "select": None, "write": None, "verify": None},
        "rc": None,
        "error": None,
    }
    try:
        envir_root = placement_envir_root(side, sc)
        db = placement_characters_db(side, sc)
        res["db"] = db
        res["db_pairing"] = _check_db_pairing(side, envir_root, db)
        client = placement_db_client(prof, side)
    except ConfigError as e:
        res["error"] = "魔法夹具前置校验失败：%s" % e
        res["rc"] = 2
        return res
    chr_name = str(prof.get("char_name") or "")
    login_id = str(prof.get("account") or "")
    if not chr_name or not login_id:
        res["error"] = ("魔法夹具需要 profile 的 char_name/account（定位 characters 行）；"
                        "当前 char_name=%r account=%r" % (chr_name, login_id))
        res["rc"] = 2
        return res
    argv = _mysql_argv(client)

    def sql(tag, text):
        rc, out, err = runner.run(side, tag, argv, env=env, timeout=timeout,
                                  stdin_text=text, capture=True)
        if rc != 0:
            raise ConfigError("%s 执行失败（mysql rc=%s）：%s"
                              % (tag, rc, (err or out or "").strip()[:400]))
        return out

    try:
        cols_out = sql("magic.columns", "/* magic */ SHOW COLUMNS FROM `%s`.`characters`;\n" % db)
        columns = [ln.split("\t")[0] for ln in cols_out.splitlines() if ln.strip()]
        missing = [c for c in ("Id", "ChrName", "LoginID") if c not in columns]
        if missing:
            raise ConfigError("%s.characters 缺列 %s：表结构与预期不符" % (db, missing))
        char_sql = _magic_char_sql(db, chr_name, login_id, "Deleted" in columns)
        res["sql"]["character"] = char_sql.strip()
        parts = (sql("magic.character", char_sql) or "").strip().split("\t")
        try:
            count = int(parts[0])
            player_id = int(parts[1])
            deleted = int(parts[2]) if len(parts) > 2 else 0
        except (ValueError, IndexError):
            raise ConfigError("magic.character 返回无法解析：%r" % (parts,))
        if count == 0:
            raise ConfigError("角色不存在：%s.characters 无 ChrName=%r LoginID=%r 的行"
                              % (db, chr_name, login_id))
        if count > 1:
            raise ConfigError("角色行不唯一：ChrName=%r LoginID=%r 命中 %d 行"
                              % (chr_name, login_id, count))
        if deleted:
            raise ConfigError("角色已删除（Deleted=%d）：ChrName=%r" % (deleted, chr_name))
        res["player_id"] = player_id
        ids = [m["magic_id"] for m in magic]
        dup_out = sql("magic.duplicate", _magic_dup_sql(db, player_id, ids))
        if (dup_out or "").strip():
            raise ConfigError("同一角色同一 MagicId 有多行（%s）——夹具不替你合并历史脏数据"
                              % dup_out.strip().replace("\n", "; "))
        rows_sql = _magic_rows_sql(db, player_id, ids)
        res["sql"]["select"] = rows_sql.strip()
        before = _parse_magic_rows(sql("magic.read", rows_sql))
        res["before"] = _magic_snapshot(magic, before)
        fp_sql = _magic_fingerprint_sql(db, player_id, ids)
        fp_before = (sql("magic.fingerprint", fp_sql) or "").strip()
        writes = []
        for m in magic:
            cur = before.get(m["magic_id"])
            exists = cur is not None
            if exists and all(cur[k] == m[k] for k in ("level", "use_key", "curr_train")):
                continue
            text = _magic_write_sql(db, player_id, m, exists)
            writes.append(text.strip())
            sql("magic.write", text)
        res["sql"]["write"] = "\n".join(writes) if writes else None
        res["action"] = "written" if writes else "noop"
        res["sql"]["verify"] = rows_sql.strip()
        after = _parse_magic_rows(sql("magic.verify", rows_sql))
        res["after"] = _magic_snapshot(magic, after)
        for m in magic:
            got = after.get(m["magic_id"])
            want = {k: m[k] for k in ("level", "use_key", "curr_train")}
            if got != want:
                raise ConfigError(
                    "魔法夹具读回不符：MagicId=%d 期望 %r 实际 %r"
                    % (m["magic_id"], want, got))
        fp_after = (sql("magic.fingerprint", fp_sql) or "").strip()
        if fp_after != fp_before:
            raise ConfigError(
                "魔法夹具连带改了**未声明的魔法行**（其它行的行数/MD5 指纹变了：%r -> %r）"
                % (fp_before or "<空>", fp_after or "<空>"))
        res["rc"] = 0
        return res
    except ConfigError as e:
        res["error"] = "魔法夹具失败：%s" % e
        res["rc"] = 2
        return res


def preview_inventory(runner, side, prof, sc, items, ctx):
    """`--dry-run` 用：背包夹具的离线校验 + 命令预览（卡 `openmir2-inventory-fixture-reachability`）。

    与 [`preview_magic`] 同口径：只做离线内容校验（库名/库配对/客户端参数），**不连库、不改库**；
    前置不成立 ⇒ 返回 failed=True（真跑会 rc=2 并点名）。预览行写「声明 N 件 / 槽位 / MakeIndex」
    ——此前 dry-run 对 inventory **一行不打**（rc=0 但 0 行 = 假绿）。
    """
    out = {"side": side, "declared": [dict(i) for i in items], "rc": None,
           "error": None, "commands": [], "offline_check": None}
    try:
        envir_root = placement_envir_root(side, sc)
        db = placement_characters_db(side, sc)
        pairing = _check_db_pairing(side, envir_root, db)
        client = placement_db_client(prof, side)
    except ConfigError as e:
        out["error"] = "背包夹具前置校验失败（真跑会 rc=2）：%s" % e
        out["rc"] = 2
        return out, True
    positions = sorted(int(i["position"]) for i in items)
    makes = sorted(int(i["make_index"]) for i in items)
    chr_name = str(prof.get("char_name") or "")
    login_id = str(prof.get("account") or "")
    argv = _redact_argv(_mysql_argv(client))
    out["offline_check"] = {"envir_root": envir_root, "db": db, "db_pairing": pairing,
                            "positions": positions, "make_index": makes,
                            "char_name": chr_name}
    out["commands"] = [
        {"argv": argv,
         "stdin": "SELECT COUNT(*), MIN(`Id`), `Deleted` FROM `%s`.`characters` "
                  "WHERE ChrName=%r AND LoginID=%r;" % (db, chr_name, login_id)},
        {"argv": argv,
         "stdin": "INSERT/UPDATE `%s`.`characters_bagitem`（声明 %d 件：positions=%s "
                  "make_index=%s）" % (db, len(items), positions, makes)},
        {"argv": argv,
         "stdin": "SELECT ...（读回校验：声明槽位行数 + 未声明槽位行数/MD5 指纹）"},
    ]
    runner.log("[%s] 背包夹具（dry-run 不连库）：db=%s 声明 %d 件 positions=%s "
               "make_index=%s 角色=%s"
               % (side, db, len(items), positions, makes, chr_name))
    out["rc"] = 0
    return out, False


def preview_magic(runner, side, prof, sc, magic, ctx):
    """`--dry-run` 用：离线校验声明与库配对，打印将要执行的命令（不连库/不改库）。"""
    out = {"side": side, "declared": [dict(m) for m in magic], "rc": None,
           "error": None, "commands": [], "offline_check": None}
    try:
        envir_root = placement_envir_root(side, sc)
        db = placement_characters_db(side, sc)
        pairing = _check_db_pairing(side, envir_root, db)
        client = placement_db_client(prof, side)
    except ConfigError as e:
        out["error"] = "魔法夹具前置校验失败（真跑会 rc=2）：%s" % e
        out["rc"] = 2
        return out, True
    ids = [m["magic_id"] for m in magic]
    id_list = ", ".join(str(x) for x in ids)
    out["offline_check"] = {"envir_root": envir_root, "db": db, "db_pairing": pairing,
                            "magic_ids": ids}
    argv = _redact_argv(_mysql_argv(client))
    chr_name = str(prof.get("char_name") or "")
    login_id = str(prof.get("account") or "")
    out["commands"] = [
        {"argv": argv,
         "stdin": "SELECT COUNT(*), MIN(`Id`), `Deleted` FROM `%s`.`characters` "
                  "WHERE ChrName=%r AND LoginID=%r;" % (db, chr_name, login_id)},
        {"argv": argv,
         "stdin": "SELECT `MagicId`,`Level`,`UseKey`,`CurrTrain` FROM "
                  "`%s`.`characters_magic` WHERE `PlayerId`=<id> AND `MagicId` IN (%s);"
                  % (db, id_list)},
        {"argv": argv,
         "stdin": "INSERT/UPDATE `%s`.`characters_magic`（声明 %d 条：%s）"
                  % (db, len(ids), id_list)},
        {"argv": argv,
         "stdin": "SELECT ... （读回校验：声明的 MagicId/Level/UseKey/CurrTrain + "
                  "未声明魔法行的行数/MD5 指纹）"},
    ]
    runner.log("[%s] 魔法夹具（dry-run 不连库）：db=%s magic_ids=%s"
               % (side, db, id_list))
    out["rc"] = 0
    return out, False


# 背包夹具（inventory）：restore 之后、起栈之前把指定物品写进测试库 characters_bagitem
#
# 卡 `openmir2-window-inventory-fixture`。选型与预置位（placement）一致：
#   (a) DB 改测试库 `characters_bagitem`   <= 本实现
#   (b) 脚本 GIVE 造物品 —— 不可行：脚本在登录后才跑，probe 的 discovery/click 早于它
#   (c) GM `@造物` —— 不可行：Permission 恒 0
# 预置必须发生在 restore 之后（restore 会复位库）且起栈之前（角色记录只在登录时读一次）。
#
# 卡 `openmir2-window-inventory-fixture-adapt`：基线实表是 **PlayerId 键**——
# DESCRIBE mir2_db.characters_bagitem（mir2_test_db / mir2_test_db_r 同构）：
#   Id(bigint PK auto) / PlayerId(bigint) / Position / MakeIndex / StdIndex / Dura / DuraMax
# 没有 `ChrName` / `LoginID` / `Index` 列（首版夹具凭空预期了这三列，真库加载期必拒——
# window-run-1 实锤 bag_equip/eat_item 15s 早退）。替代键方案：`PlayerId` =
# `characters.Id`，由 profile 的 char_name/account 在 `characters` 表上解析
# （ChrName+LoginID 唯一、未删除，与魔法夹具 `_magic_char_sql` 同口径）；物品模板号
# 写 `StdIndex` 列，模板行按 `stditems.Id` 对（实表没有 `idx` 列）。
# 两侧 db-srv 读包语义即 `SELECT * FROM characters_bagitem WHERE PlayerId=?`
# （`mir2-rs/bins/db-srv/src/playdata.rs` get_item_slots），与 oracle 同键。
#
# 声明形态（场景顶层）：
#   "inventory": [{"index": 100, "make_index": 5001, "position": 3,
#                  "std_mode": 0, "ani_count": 0, "dura": 0, "dura_max": 0}]
# - `index`（物品模板号）与 `position`（0..45 槽位）、`make_index` 必填；
# - `std_mode`/`ani_count` 声明时**必须与 `stditems` 模板一致**（不符 ⇒ rc=2，防止
#   「以为是药水结果造出武器」）；
# - 两侧 `make_index` 必须相同（`make_index` 是会话内变量，跨侧不同会 value_mismatch）。
# ---------------------------------------------------------------------------
INVENTORY_REQUIRED_KEYS = ("index", "make_index", "position")
INVENTORY_OPTIONAL_KEYS = ("std_mode", "ani_count", "dura", "dura_max", "note")
INVENTORY_MAX_BAG = 46


def _pick_occupancy_pred(text):
    """从 `characters_bagitem` 的占用 SQL 里取出**占用谓词**的内容（不含外层括号）。

    卡 `openmir2-cardwindow-occupancy-parser-safety`（reviewer-21 在 `0ef97e49` 的 N2）：
    旧实现用裸 `text.rfind("AND (")`——**认的是「最后一组」而不是「占用那组」**。SQL 将来
    多加一个 AND 组（例如 `AND (` + BT + 'Position' + BT + ' <> -1)`）就会静默改去求值新那组：
    两颗占用牙仍然绿，却已不再钉占用规则。

    规则：从右往左逐个 `AND (` 候选，**只认 body 里同时含 `StdIndex` 与 `MakeIndex` 的那组**
    （占用谓词就是 `StdIndex <> 0 OR MakeIndex <> 0` 这两列；只认 `StdIndex` 会被
    「只提 StdIndex 的新组」骗走），并按括号配平截取
    （谓词里再套一层括号也截得对）。一个都找不到 ⇒ 返回 None——调用方按「数所有行」退化成
    修复前的语义，空占位行会被算成占用 ⇒ ⑲ 立刻红，**不静默**。
    """
    idx = len(text)
    while True:
        i = text.rfind("AND (", 0, idx)
        if i < 0:
            return None
        start = text.find("(", i)
        depth = 0
        cand = None
        for j in range(start, len(text)):
            ch = text[j]
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0:
                    cand = text[start + 1:j]
                    break
        # 守卫要**两个列名都在**：占用谓词是 `StdIndex … OR MakeIndex …`；
        # 只认 `StdIndex` 的话，将来一个只提 `StdIndex` 的新组（如 `AND (`StdIndex` = 0)`）
        # 会被当成占用谓词——那也是一种静默换目标。
        if cand is not None and "StdIndex" in cand and "MakeIndex" in cand:
            return cand
        idx = i


def _selftest_inventory_reachability(tmp):
    """卡 `openmir2-inventory-fixture-reachability`：让 inventory 夹具**真的可达**的三条断言。

    ① 仓库里的 `bag_equip` / `eat_item` 已经带 `inventory` 声明且能解析；
    ② `run_inventory` 在 dry-run 下**打印预览行**并 rc=0（此前 rc=0 但 0 行 = 假绿）；
    ③ `mir2_probe --check-scenario` 接受带 `inventory` 的场景（此前 unknown key ⇒ rc=2，场景无法带声明）。
    """
    import json as _json
    out = []
    here = os.path.dirname(os.path.abspath(__file__))
    scen_dir = os.path.join(here, "..", "probe", "scenarios")
    decl = {}
    have_replace = {}
    bad_decl = {}
    prof_dir = os.path.join(here, "..", "window", "profiles")
    for name in ("bag_equip", "eat_item"):
        # 卡 `openmir2-scenario-fixture-migration` F2：**「没有 inventory」与「inventory 声明坏」
        # 必须分开**——前者迁移后合法（改由 profile 的 fixtures.inventory_replace），后者是真错，
        # 混成同一个 None 会让自查牙变钝。
        try:
            decl[name] = scenario_inventory(os.path.join(scen_dir, name + ".json"))
        except ConfigError as e:
            decl[name] = None
            bad_decl[name] = str(e)
        # 卡 `openmir2-scenario-fixture-migration`：迁移后这两条改由 profile 的
        # `fixtures.inventory_replace` 声明（旧 `inventory` 通道在 46/46 满包上 fail-closed）。
        have_replace[name] = False
        pf = os.path.join(prof_dir, name + ".json")
        if os.path.isfile(pf):
            try:
                with open(pf, "r", encoding="utf-8-sig") as f:
                    pdoc = json.load(f)
                fx = pdoc.get("fixtures") or {}
                have_replace[name] = bool(fx.get("inventory_replace"))
            except (OSError, ValueError):
                have_replace[name] = False
    out.append((not bad_decl
                and (bool(decl.get("bag_equip")) or have_replace["bag_equip"])
                and (bool(decl.get("eat_item")) or have_replace["eat_item"]),
                "① bag_equip/eat_item 声明了 inventory 或 profile 的 fixtures.inventory_replace；"
                "且没有坏的 inventory 声明（%s）" % (sorted(bad_decl) or "无")))

    class _DryRunner:
        dry_run = True
        def __init__(self):
            self.lines = []
            self.sides = []
        def log(self, text):
            self.lines.append(text)
        def run(self, *a, **k):
            self.sides.append(a[1] if len(a) > 1 else "?")
            return 0, "", ""

    logs = []
    ok_dry = True
    prof = {"char_name": "hero", "account": "ua",
            "sides": {"csharp": {}, "rust": {}},
            "db_client": {"mysql": ["mysql"]}}
    try:
        cur = {"db": {"characters_db": "mir2_test_db"}, "envir_root": None}
        old_root, old_pair = placement_envir_root, _check_db_pairing
        globals()["placement_envir_root"] = lambda side, sc: None
        globals()["_check_db_pairing"] = lambda *a, **k: "selftest"
        try:
            for side in ("csharp", "rust"):
                dr = _DryRunner()
                synth = [{"index": 207, "make_index": 5001, "position": 40,
                          "std_mode": 5, "ani_count": 0, "dura": 4000,
                          "dura_max": 4000, "note": "selftest 合成声明"}]
                res, failed = preview_inventory(dr, side, prof, cur, synth, {})
                logs.extend(dr.lines)
                if failed or res.get("rc") != 0:
                    ok_dry = False
        finally:
            globals()["placement_envir_root"] = old_root
            globals()["_check_db_pairing"] = old_pair
    except Exception:
        ok_dry = False
    out.append((ok_dry and any("背包夹具（dry-run 不连库）" in l for l in logs),
                "② dry-run 两侧打印「背包夹具（dry-run 不连库）」预览且 rc=0"))

    # ④ `inventory_replace` 的 dry-run 预览（卡 `openmir2-scenario-fixture-migration`）：
    #    此前只有 selftest 调用 `run_inventory_fixture` 的 dry-run 分支，主 CLI dry-run 一行不打。
    logs4 = []
    ok4 = True
    try:
        cur = {"db": {"characters_db": "mir2_test_db"}, "envir_root": None}
        for side in ("csharp", "rust"):
            dr = _DryRunner()
            res = run_inventory_fixture(dr, side, prof, cur,
                                        {"inventory_replace": [{"position": 40, "index": 207,
                                                                "make_index": 5001}]},
                                        {}, None)
            logs4.extend(dr.lines)
            if res.get("rc") != 0:
                ok4 = False
    except Exception:
        ok4 = False
    out.append((ok4 and any("背包替换/丢弃夹具（dry-run 不连库）" in l for l in logs4)
                and any("replace Position=40" in l for l in logs4),
                "④ dry-run 打印 inventory_replace 预览（replace Position=40）且 rc=0"))

    # ③ probe 加载放行：临时场景带 inventory + 必需 --var ⇒ rc=0
    import subprocess, sys as _sys, tempfile
    probe = os.path.join(here, "..", "probe", "mir2_probe.py")
    body = {"name": "selftest_inv_load", "required": ["make_index", "takeon_where",
                                                      "item_name"],
            "inventory": [{"index": 207, "make_index": 5001, "position": 40, "std_mode": 5}],
            "steps": [{"send": {"ident": 3030, "text": "@时间"}},
                      {"wait_for": 100}]}
    fd, path = tempfile.mkstemp(suffix=".json")
    os.close(fd)
    try:
        with open(path, "w", encoding="utf-8") as f:
            _json.dump(body, f)
        try:
            r = subprocess.run([_sys.executable, "-I", "-B", probe, "--check-scenario",
                                "--scenario", path, "--var", "make_index=5001",
                                "--var", "takeon_where=1", "--var", "item_name=x"],
                               capture_output=True, text=True, stdin=subprocess.DEVNULL,
                               timeout=120)
            rc_probe = r.returncode
        except subprocess.TimeoutExpired:
            rc_probe = 124
        out.append((rc_probe == 0, "③ mir2_probe --check-scenario 接受 inventory 声明（rc=%s）"
                    % rc_probe))
    finally:
        try:
            os.unlink(path)
        except OSError:
            pass
    return out


def scenario_inventory(path):
    """读场景顶层 `inventory` 声明。没有 => None；形状非法 => ConfigError（fail-closed）。"""
    try:
        with open(path, "r", encoding="utf-8-sig") as f:
            data = json.load(f)
    except (OSError, ValueError) as e:
        raise ConfigError("%s: cannot load scenario: %s" % (path, e))
    if not isinstance(data, dict):
        raise ConfigError("%s: scenario must be a JSON object" % path)
    inv = data.get("inventory")
    if inv is None:
        return None
    if not isinstance(inv, list) or not inv:
        raise ConfigError("%s: scenario.inventory 必须是非空数组" % path)
    out = []
    seen_pos = set()
    seen_make = set()
    for i, item in enumerate(inv):
        where = "%s: scenario.inventory[%d]" % (path, i)
        if not isinstance(item, dict):
            raise ConfigError("%s 必须是对象" % where)
        bad = set(item) - set(INVENTORY_REQUIRED_KEYS) - set(INVENTORY_OPTIONAL_KEYS)
        if bad:
            raise ConfigError("%s 有未知键 %s（fail-closed）" % (where, sorted(bad)))
        missing = [k for k in INVENTORY_REQUIRED_KEYS if k not in item]
        if missing:
            raise ConfigError("%s 缺必填键 %s" % (where, missing))
        for key in ("index", "make_index", "position", "std_mode", "ani_count",
                    "dura", "dura_max"):
            v = item.get(key)
            if v is None:
                continue
            if not isinstance(v, int) or isinstance(v, bool):
                raise ConfigError("%s.%s 必须是整数（实际 %r）" % (where, key, v))
            if v < 0 or v > 0xFFFFFFFF:
                raise ConfigError("%s.%s=%r 越界（0..4294967295）" % (where, key, v))
        pos = int(item["position"])
        if pos >= INVENTORY_MAX_BAG:
            raise ConfigError("%s.position=%d 越界（0..%d）"
                              % (where, pos, INVENTORY_MAX_BAG - 1))
        if pos in seen_pos:
            raise ConfigError("%s.position=%d 与前面的条目重复（同一槽位只能声明一次）"
                              % (where, pos))
        seen_pos.add(pos)
        mk = int(item["make_index"])
        if mk <= 0:
            raise ConfigError("%s.make_index 必须 >0（0 会被 DBSvr 整行跳过）" % where)
        if mk in seen_make:
            raise ConfigError("%s.make_index=%d 重复（两侧/同侧都必须唯一）" % (where, mk))
        seen_make.add(mk)
        if item.get("note") is not None and not isinstance(item["note"], str):
            raise ConfigError("%s.note 必须是字符串" % where)
        out.append({
            "index": int(item["index"]),
            "make_index": mk,
            "position": pos,
            "std_mode": item.get("std_mode"),
            "ani_count": item.get("ani_count"),
            "dura": int(item.get("dura") or 0),
            "dura_max": int(item.get("dura_max") or 0),
            "note": item.get("note") or "",
        })
    return out


def inventory_fingerprint_sql(db, player_id, columns, exclude_positions=()):
    """读该角色**其它槽位**的快照：行数 + 除声明列外其余列的 MD5 指纹。

    口径（review `c866f358` M-1）：预置只会写声明的槽位，所以「未声明的行有没有被
    动过」必须**排除声明槽位**来比 —— 否则新增行本身就会让聚合指纹变化，证明不了任何事。
    `exclude_positions` 非空时加 `Position NOT IN (…)`。
    卡 `openmir2-window-inventory-fixture-adapt`：行键是 `PlayerId`（实表无 ChrName/LoginID）。
    """
    skip = {"Id", "PlayerId", "MakeIndex", "Position", "StdIndex", "Dura", "DuraMax"}
    fp_cols = [c for c in columns if c not in skip]
    # 实表的列可能**全部**属于声明列（DESCRIBE mir2_db.characters_bagitem 就是这 7 列）；
    # 此时没有可指纹的余列，退化为只比行数（`CONCAT_WS('')` 是单参调用，MySQL 直接报错）。
    fp = ("COALESCE(MIN(MD5(CONCAT_WS(%s))), '')"
          % ", CHAR(31), ".join("IFNULL(CAST(`%s` AS CHAR), '~')" % c for c in fp_cols)
          ) if fp_cols else "''"
    excl = ""
    if exclude_positions:
        excl = " AND `Position` NOT IN (%s)" % ", ".join(str(int(x)) for x in exclude_positions)
    sql = (
        "SELECT COUNT(*), %s\n"
        "FROM `%s`.`characters_bagitem` WHERE `PlayerId` = %d%s;\n"
        % (fp, db, int(player_id), excl))
    return sql


def _inventory_char_sql(db, chr_name, login_id, has_deleted):
    """按 ChrName+LoginID 在 `characters` 上解析角色（卡 `openmir2-window-inventory-fixture-adapt`）。

    返回 COUNT(*), MIN(Id)（, Deleted——表无该列时省略，调用方按 0 处理）；
    与 `_magic_char_sql` 同口径，仅去掉 magic 注释前缀。
    """
    tail = ", `Deleted`" if has_deleted else ""
    return ("/* inventory */ SELECT COUNT(*), COALESCE(MIN(`Id`), 0)%s FROM `%s`.`characters` "
            "WHERE `ChrName` = %s AND `LoginID` = %s;\n"
            % (tail, db, _sql_quote(chr_name), _sql_quote(login_id)))


def validate_inventory_content(items):
    """离线校验：条目自身合法 + 两侧 `make_index` 相同（跨侧检查由调用方传入两组）。"""
    if not items:
        return
    pos = [it["position"] for it in items]
    if len(set(pos)) != len(pos):
        raise ConfigError("inventory: 同一槽位声明了多次：%r" % sorted(pos))
    mk = [it["make_index"] for it in items]
    if len(set(mk)) != len(mk):
        raise ConfigError("inventory: MakeIndex 有重复：%r" % sorted(mk))


def compare_inventory_make_index(side_items, other_side, other_items):
    """两侧 `MakeIndex` 必须逐条相同（卡面 acceptance ③）；否则 rc=2 点名差异。"""
    a = sorted(it["make_index"] for it in (side_items or []))
    b = sorted(it["make_index"] for it in (other_items or []))
    if a != b:
        raise ConfigError(
            "inventory: %s 与本侧声明的 MakeIndex 不同（%r vs %r）——两侧会 value_mismatch，"
            "声明里请用相同的固定值" % (other_side, b, a))


def run_inventory(runner, side, prof, sc, items, env, timeout=120):
    """在一侧执行背包夹具：restore 之后、起栈之前。返回证据 dict（rc=0 才算完成）。

    fail-closed：目标角色不存在/多行/已删除、`stditems` 查不到或与声明的 std_mode/ani_count
    不符、写后读回不符、指纹变了 ⇒ rc=2 点名。行键 `PlayerId`（= characters.Id）见
    `openmir2-window-inventory-fixture-adapt`。
    """
    res = {"side": side, "declared": items, "db": None, "player_id": None,
           "action": None, "before": None, "after": None, "altered_columns": [],
           "sql": {}, "rc": None, "error": None}
    if not items:
        res["action"] = "noop"
        res["rc"] = 0
        return res
    try:
        db = placement_characters_db(side, sc)
        res["db"] = db
        # review `c866f358` I-2：与 `run_placement` 同一口径——用部署 `Server.conf` 的
        # [DataBase] database 交叉核对 profile 的角色库（拿错库预置会变成假差异）。
        res["db_pairing"] = _check_db_pairing(side, placement_envir_root(side, sc), db)
        client = placement_db_client(prof, side)
    except ConfigError as e:
        res["error"] = "背包夹具前置校验失败：%s" % e
        res["rc"] = 2
        return res
    argv = _mysql_argv(client)
    chr_name = str(prof.get("char_name") or "")
    login_id = str(prof.get("account") or "")
    if not chr_name or not login_id:
        res["error"] = "背包夹具需要 profile 的 char_name/account"
        res["rc"] = 2
        return res
    if getattr(runner, "dry_run", False):
        # 卡 `openmir2-inventory-fixture-reachability`：dry-run 必须**看得到预览行**——
        # 此前 `run_inventory` 只在真跑分支里打日志，`--dry-run` 下 0 行（rc=0 假绿）。
        positions = sorted(int(i["position"]) for i in items)
        makes = sorted(int(i["make_index"]) for i in items)
        res["action"] = "dry-run"
        res["sql"] = {
            "declare": "INSERT/UPDATE `%s`.`characters_bagitem`（声明 %d 件：positions=%s "
                       "make_index=%s）" % (db, len(items), positions, makes),
        }
        runner.log("[%s] 背包夹具（dry-run 不连库）：db=%s 声明 %d 件 positions=%s "
                   "make_index=%s 角色=%s"
                   % (side, db, len(items), positions, makes, chr_name))
        res["rc"] = 0
        return res

    def sql(tag, text):
        rc, out, err = runner.run(side, tag, argv, env=env, timeout=timeout,
                                  stdin_text=text, capture=True)
        if rc != 0:
            raise ConfigError("%s 执行失败（mysql rc=%s）：%s"
                              % (tag, rc, (err or out or "").strip()[:400]))
        return out

    try:
        cols_out = sql("inventory.columns",
                       "SHOW COLUMNS FROM `%s`.`characters_bagitem`;\n" % db)
        columns = [ln.split("\t")[0] for ln in cols_out.splitlines() if ln.strip()]
        # 卡 `openmir2-window-inventory-fixture-adapt`：实表（DESCRIBE mir2_db.characters_bagitem，
        # mir2_test_db / mir2_test_db_r 同构）是 PlayerId 键 + StdIndex；首版预期的
        # ChrName/LoginID/Index 三列在基线库不存在（window-run-1 §2 第 10/11 行实锤）。
        for need in ("PlayerId", "Position", "MakeIndex", "StdIndex", "Dura", "DuraMax"):
            if need not in columns:
                raise ConfigError("%s.characters_bagitem 缺列 %s（表结构与预期不符，拒绝）"
                                  % (db, need))
        # 替代键：`PlayerId` = `characters.Id`，按 profile 的 char_name/account 解析
        # （ChrName+LoginID 唯一、未删除——与魔法夹具 `_magic_char_sql` 同口径）。
        char_cols_out = sql("inventory.character.columns",
                            "SHOW COLUMNS FROM `%s`.`characters`;\n" % db)
        char_columns = [ln.split("\t")[0] for ln in char_cols_out.splitlines() if ln.strip()]
        char_missing = [c for c in ("Id", "ChrName", "LoginID") if c not in char_columns]
        if char_missing:
            raise ConfigError("%s.characters 缺列 %s：表结构与预期不符"
                              % (db, char_missing))
        char_sql = _inventory_char_sql(db, chr_name, login_id, "Deleted" in char_columns)
        res["sql"]["character"] = char_sql.strip()
        parts = (sql("inventory.character", char_sql) or "").strip().split("\t")
        try:
            count = int(parts[0])
            player_id = int(parts[1])
            deleted = int(parts[2]) if len(parts) > 2 else 0
        except (ValueError, IndexError):
            raise ConfigError("inventory.character 返回无法解析：%r" % (parts,))
        if count == 0:
            raise ConfigError("角色不存在：%s.characters 无 ChrName=%r LoginID=%r 的行"
                              % (db, chr_name, login_id))
        if count > 1:
            raise ConfigError("角色行不唯一：ChrName=%r LoginID=%r 命中 %d 行"
                              % (chr_name, login_id, count))
        if deleted:
            raise ConfigError("角色已删除（Deleted=%d）：ChrName=%r" % (deleted, chr_name))
        res["player_id"] = player_id
        positions = [it["position"] for it in items]
        # 声明槽位必须原本为空：DELETE+INSERT 会把该行其它列重置成默认值，工具不替操作者
        # 做「覆盖已有物品」这件事（review `c866f358` M-1）。
        # 卡 `openmir2-window-run3-prereq-triage`（reviewer-21 `3dc30ae5` 返工，n17 真因）：
        # **占用 = 这一格真有物品**，不是「有行」。存档约定是每槽一行、清空 = 把列置零
        #（基线 `characters_bagitem` 对每个角色恒有 46 行 `Position 0..45`，全零占位），
        # 只 `COUNT(*) WHERE Position IN` 会把**整包**判成已占用 ⇒ 46/46 基线上任何声明都
        # fail-closed（run3 #17 实测：「声明槽位里已有 2 行（[40, 41]）」而两行都是空占位）。
        # `MakeIndex` 是带服务器号的合成键（DBSvr 写入必非 0；空行恒 0），所以
        # `StdIndex <> 0 OR MakeIndex <> 0` 与「这一格真有物品」等价。
        occ_sql = ("SELECT COUNT(*) FROM `%s`.`characters_bagitem`\n"
                   " WHERE `PlayerId` = %d AND `Position` IN (%s)\n"
                   "   AND (`StdIndex` <> 0 OR `MakeIndex` <> 0);\n"
                   % (db, player_id,
                      ", ".join(str(p) for p in positions)))
        occ = sql("inventory.occupancy", occ_sql)
        occ_n = int((occ or "0").strip().splitlines()[0] or 0)
        res["pre_occupied"] = occ_n
        if occ_n != 0:
            raise ConfigError(
                "inventory: 声明槽位里已有 %d 行（%s）——DELETE+INSERT 会把该行其它列重置，"
                "请改用空槽或先清空" % (occ_n, positions))
        fp_sql = inventory_fingerprint_sql(db, player_id, columns,
                                           exclude_positions=positions)
        res["sql"]["read"] = fp_sql.strip()
        before = _parse_inventory_fp(sql("inventory.read", fp_sql))
        res["before"] = before
        # 每件：模板必须存在且与声明一致
        values = []
        for it in items:
            tpl = sql("inventory.template",
                      "SELECT `StdMode`, `Shape`, `DuraMax` FROM `mir2_data`.`stditems`\n"
                      " WHERE `Id` = %d;\n" % it["index"]).strip().splitlines()
            if not tpl:
                raise ConfigError("inventory: 物品模板 index=%d 不存在（mir2_data.stditems）"
                                  % it["index"])
            parts = tpl[0].split("\t")
            std_mode = int(parts[0]) if parts and parts[0] not in ("", "NULL") else None
            if it.get("std_mode") is not None and std_mode != it["std_mode"]:
                raise ConfigError(
                    "inventory: index=%d 的 StdMode=%r 与声明 %r 不符"
                    % (it["index"], std_mode, it["std_mode"]))
            values.append("(%d, %d, %d, %d, %d, %d)"
                          % (player_id, it["position"], it["make_index"], it["index"],
                             it["dura"], it["dura_max"]))
        upd = ("DELETE FROM `%s`.`characters_bagitem`\n"
               " WHERE `PlayerId` = %d AND `Position` IN (%s);\n"
               "INSERT INTO `%s`.`characters_bagitem`\n"
               " (`PlayerId`, `Position`, `MakeIndex`, `StdIndex`, `Dura`, `DuraMax`)\n"
               " VALUES %s;\n"
               % (db, player_id,
                  ", ".join(str(it["position"]) for it in items), db, ", ".join(values)))
        res["sql"]["update"] = upd.strip()
        sql("inventory.update", upd)
        written = int((sql("inventory.verify_rows", occ_sql) or "0").strip()
                      .splitlines()[0] or 0)
        res["written"] = written
        check_inventory_rows(items, written)
        after = _parse_inventory_fp(sql("inventory.verify", fp_sql))
        res["after"] = after
        res["action"] = "updated" if written >= len(items) else None
        # 「只写了声明的槽位」的机判：其它槽位（排除声明位置）的行数与指纹都必须逐字节不变。
        if after["count"] != before["count"]:
            raise ConfigError(
                "inventory: 其它槽位的行数变了（%d → %d）——预置动到了未声明的行"
                % (before["count"], after["count"]))
        if after["fp"] != before["fp"]:
            res["altered_columns"] = ["<其它槽位的指纹变化>"]
            raise ConfigError("inventory: 其它槽位的指纹变了（%r → %r）"
                              % (before["fp"], after["fp"]))
        res["rc"] = 0
        return res
    except ConfigError as e:
        res["error"] = "背包夹具失败：%s" % e
        res["rc"] = 2
        return res


def check_inventory_rows(items, after_count):
    """写后行数必须覆盖声明件数；少了 ⇒ rc=2（牙「跳过预置」的红条件）。"""
    if int(after_count) < len(items):
        raise ConfigError("inventory: 写后行数 %d 少于声明件数 %d" % (after_count, len(items)))


def _parse_inventory_fp(out):
    lines = [ln for ln in (out or "").splitlines() if ln.strip()]
    if len(lines) != 1:
        raise ConfigError("背包指纹读行不是 1 行（实际 %d）：%r" % (len(lines), out[:200]))
    parts = lines[0].split("\t")
    if len(parts) != 2:
        raise ConfigError("背包指纹读行列数不是 2：%r" % lines[0][:200])
    try:
        return {"count": int(parts[0]), "fp": parts[1]}
    except ValueError as e:
        raise ConfigError("背包指纹无法解析：%r（%s）" % (lines[0][:200], e))


# ---------------------------------------------------------------------------
# 背包替换/丢弃夹具（`fixtures.inventory_replace` / `fixtures.inventory_drop`）
#
# 卡 `openmir2-window-fixture-inventory`。缺口（dev-1 butch 实测）：基线 `winplay0` 背包
# **46/46 满**且无武器 ⇒ 既有场景顶层 `inventory` 通道（要求空槽、fail-closed）写不进去，
# 「装武器打怪」（`CM_TAKEONITEM`）类前置无法声明。
#
# 本通道走**既有 `fixtures`**（与 `level` 同款）：
#   - 声明期：`_validate_fixture_decl` 校验形状/越界/重复；`_check_fixture_symmetry`
#     要求两侧声明逐字节相同（否则两侧基线不同，parity 无意义）；
#   - 运行期（restore 之后、起栈之前）：逐槽「写前快照整行 → DELETE(+INSERT) → 读回断言」，
#     并对**未声明槽位**做行数 + MD5 指纹对比（只许动声明槽）；
#   - 停栈后 `restore_inventory_fixture` 按快照**整行**（含 `Id`）写回并重读校验；
#     还原失败该侧 rc=2（绝不静默跳过）。
#
# 语义要点：
#   - `inventory_replace`: 覆盖/新增某槽（原有行被快照，整行还原）；
#   - `inventory_drop`: 清空某槽；**该槽本来为空即报错**（夹具没生效，不许假绿）。
#   - `std_mode` / `ani_count` 若声明，则与 `mir2_data.stditems` 的 `StdMode` / `Shape`
#     交叉核对；`index`（= `StdIndex`）必须存在于模板表。
# ---------------------------------------------------------------------------
#: 背包行整行快照的列（还原时连 `Id` 一起写回，保证行与指纹逐字节复原）。
INVENTORY_FIXTURE_ROW_COLUMNS = ("Id", "Position", "MakeIndex", "StdIndex", "Dura", "DuraMax")


def _fixture_inventory_ops(decl):
    """把 fixtures.inventory_replace / inventory_drop 归一成有序操作列表。"""
    out = []
    for it in decl.get("inventory_replace") or []:
        out.append({"op": "replace", "position": int(it["position"]),
                    "std_index": int(it["index"]), "make_index": int(it["make_index"]),
                    "std_mode": it.get("std_mode"), "ani_count": it.get("ani_count"),
                    "dura": int(it.get("dura") or 0),
                    "dura_max": int(it.get("dura_max") or 0),
                    "note": it.get("note") or ""})
    for it in decl.get("inventory_drop") or []:
        out.append({"op": "drop", "position": int(it["position"]),
                    "note": it.get("note") or ""})
    return out


def _inv_row_sql(db, player_id, positions):
    return ("SELECT `Position`, `Id`, `MakeIndex`, `StdIndex`, `Dura`, `DuraMax`\n"
            "FROM `%s`.`characters_bagitem`\n"
            " WHERE `PlayerId` = %d AND `Position` IN (%s)\n"
            " ORDER BY `Position`, `Id`;\n"
            % (db, int(player_id), ", ".join(str(int(p)) for p in positions)))


def _parse_inv_rows(out):
    """解析 `_inv_row_sql` 的输出；同槽多行/列数不对 => ConfigError（拒绝在有脏数据的槽上动手）。"""
    rows = {}
    for ln in (out or "").splitlines():
        if not ln.strip():
            continue
        parts = ln.split("\t")
        if len(parts) != len(INVENTORY_FIXTURE_ROW_COLUMNS):
            raise ConfigError("背包行读回列数不是 %d：%r"
                              % (len(INVENTORY_FIXTURE_ROW_COLUMNS), ln[:200]))
        try:
            row = {"Position": int(parts[0]), "Id": int(parts[1]),
                   "MakeIndex": int(parts[2]), "StdIndex": int(parts[3]),
                   "Dura": int(parts[4]), "DuraMax": int(parts[5])}
        except ValueError:
            raise ConfigError("背包行读回含非整数/NULL：%r" % ln[:200])
        pos = row["Position"]
        if pos in rows:
            raise ConfigError("Position=%d 在背包读回里出现多行（脏数据，拒绝预置）" % pos)
        rows[pos] = row
    return rows


def _inv_write_sql(db, player_id, op):
    pos = int(op["position"])
    text = ("DELETE FROM `%s`.`characters_bagitem`\n"
            " WHERE `PlayerId` = %d AND `Position` = %d;\n" % (db, int(player_id), pos))
    if op["op"] == "replace":
        text += ("INSERT INTO `%s`.`characters_bagitem`\n"
                 " (`PlayerId`, `Position`, `MakeIndex`, `StdIndex`, `Dura`, `DuraMax`)\n"
                 " VALUES (%d, %d, %d, %d, %d, %d);\n"
                 % (db, int(player_id), pos, int(op["make_index"]), int(op["std_index"]),
                    int(op["dura"]), int(op["dura_max"])))
    return text


def _inv_restore_sql(db, player_id, position, row):
    text = ("DELETE FROM `%s`.`characters_bagitem`\n"
            " WHERE `PlayerId` = %d AND `Position` = %d;\n"
            % (db, int(player_id), int(position)))
    if row:
        text += ("INSERT INTO `%s`.`characters_bagitem`\n"
                 " (`Id`, `PlayerId`, `Position`, `MakeIndex`, `StdIndex`, `Dura`, `DuraMax`)\n"
                 " VALUES (%d, %d, %d, %d, %d, %d, %d);\n"
                 % (db, int(row["Id"]), int(player_id), int(position),
                    int(row["MakeIndex"]), int(row["StdIndex"]),
                    int(row["Dura"]), int(row["DuraMax"])))
    return text


def _inv_resolve_player(runner, side, prof, sc, env, timeout):
    """解析目标库/角色行（与 `run_inventory` 同口径：ChrName+LoginID、唯一、未删除）。"""
    db = placement_characters_db(side, sc)
    pairing = _check_db_pairing(side, placement_envir_root(side, sc), db)
    client = placement_db_client(prof, side)
    argv = _mysql_argv(client)
    chr_name = str(prof.get("char_name") or "")
    login_id = str(prof.get("account") or "")
    if not chr_name or not login_id:
        raise ConfigError("背包夹具需要 profile 的 char_name/account")

    def sql(tag, text):
        rc, out, err = runner.run(side, tag, argv, env=env, timeout=timeout,
                                  stdin_text=text, capture=True)
        if rc != 0:
            raise ConfigError("%s 执行失败（mysql rc=%s）：%s"
                              % (tag, rc, (err or out or "").strip()[:400]))
        return out

    char_cols_out = sql("inventory_fixture.character.columns",
                        "SHOW COLUMNS FROM `%s`.`characters`;\n" % db)
    char_columns = [ln.split("\t")[0] for ln in char_cols_out.splitlines() if ln.strip()]
    char_missing = [c for c in ("Id", "ChrName", "LoginID") if c not in char_columns]
    if char_missing:
        raise ConfigError("%s.characters 缺列 %s：表结构与预期不符" % (db, char_missing))
    char_sql = _inventory_char_sql(db, chr_name, login_id, "Deleted" in char_columns)
    parts = (sql("inventory_fixture.character", char_sql) or "").strip().split("\t")
    try:
        count = int(parts[0])
        player_id = int(parts[1])
        deleted = int(parts[2]) if len(parts) > 2 else 0
    except (ValueError, IndexError):
        raise ConfigError("inventory_fixture.character 返回无法解析：%r" % (parts,))
    if count == 0:
        raise ConfigError("角色不存在：%s.characters 无 ChrName=%r LoginID=%r 的行"
                          % (db, chr_name, login_id))
    if count > 1:
        raise ConfigError("角色行不唯一：ChrName=%r LoginID=%r 命中 %d 行"
                          % (chr_name, login_id, count))
    if deleted:
        raise ConfigError("角色已删除（Deleted=%d）：ChrName=%r" % (deleted, chr_name))
    cols_out = sql("inventory_fixture.columns",
                   "SHOW COLUMNS FROM `%s`.`characters_bagitem`;\n" % db)
    columns = [ln.split("\t")[0] for ln in cols_out.splitlines() if ln.strip()]
    for need in ("PlayerId", "Position", "MakeIndex", "StdIndex", "Dura", "DuraMax"):
        if need not in columns:
            raise ConfigError("%s.characters_bagitem 缺列 %s（表结构与预期不符，拒绝）"
                              % (db, need))
    return {"db": db, "pairing": pairing, "client": client, "argv": argv,
            "chr_name": chr_name, "player_id": player_id, "columns": columns,
            "sql": sql, "char_sql": char_sql}


def run_inventory_fixture(runner, side, prof, sc, decl, env, side_dir, timeout=120):
    """起栈前应用 `fixtures.inventory_replace` / `fixtures.inventory_drop`（返回证据 dict）。

    fail-closed：目标角色/模板不存在、drop 空槽、写后读回不符、其它槽位指纹变化 ⇒ rc=2；
    已写项**就地回滚**。停栈后的最终还原由 `restore_inventory_fixture` 负责。
    """
    ops = _fixture_inventory_ops(decl)
    res = {"side": side,
           "declared": {"inventory_replace": decl.get("inventory_replace"),
                        "inventory_drop": decl.get("inventory_drop")},
           "action": None, "db": None, "player_id": None,
           "fingerprint": {"before": None, "after": None},
           "targets": [], "sql": {}, "rc": None, "error": None}
    if not ops:
        res["action"] = "noop"
        res["rc"] = 0
        return res
    if getattr(runner, "dry_run", False):
        res["action"] = "dry-run"
        res["sql"]["write"] = [_inv_write_sql("<db>", -1, op).strip() for op in ops]
        positions = [op["position"] for op in ops]
        runner.log("[%s] 背包替换/丢弃夹具（dry-run 不连库）：%d 项 positions=%s"
                   % (side, len(ops), positions))
        for op in ops:
            if op["op"] == "replace":
                runner.log("[%s]   replace Position=%d StdIndex=%d MakeIndex=%d"
                           % (side, op["position"], op["std_index"], op["make_index"]))
            else:
                runner.log("[%s]   drop    Position=%d" % (side, op["position"]))
        res["rc"] = 0
        return res
    try:
        ctx = _inv_resolve_player(runner, side, prof, sc, env, timeout)
    except ConfigError as e:
        res["error"] = "背包替换/丢弃夹具前置校验失败：%s" % e
        res["rc"] = 2
        return res
    # 凭据只放内存 `_session`；写盘一律经 `level_evidence()` 过滤（与 level/assets 同款）。
    res["_session"] = {"client": ctx["client"], "argv": ctx["argv"]}
    res["db"] = ctx["db"]
    res["db_pairing"] = ctx["pairing"]
    res["player_id"] = ctx["player_id"]
    sql = ctx["sql"]
    db, pid = ctx["db"], ctx["player_id"]
    positions = [op["position"] for op in ops]
    try:
        fp_sql = inventory_fingerprint_sql(db, pid, ctx["columns"],
                                           exclude_positions=positions)
        res["sql"]["read"] = fp_sql.strip()
        res["fingerprint"]["before"] = _parse_inventory_fp(
            sql("inventory_fixture.read", fp_sql))
        snap_sql = _inv_row_sql(db, pid, positions)
        res["sql"]["snapshot"] = snap_sql.strip()
        before_rows = _parse_inv_rows(sql("inventory_fixture.snapshot", snap_sql))

        # 先把全部模板/drop 前置校验完，一个字节都不写。
        for op in ops:
            ev = {"position": op["position"], "op": op["op"],
                  "before": before_rows.get(op["position"]), "after": None}
            if op["op"] == "replace":
                tpl = sql("inventory_fixture.template",
                          "SELECT `StdMode`, `Shape` FROM `mir2_data`.`stditems`\n"
                          " WHERE `Id` = %d;\n" % op["std_index"]).strip().splitlines()
                if not tpl:
                    raise ConfigError("inventory_replace: 物品模板 index=%d 不存在"
                                      "（mir2_data.stditems）" % op["std_index"])
                tparts = tpl[0].split("\t")
                try:
                    std_mode = (int(tparts[0])
                                if tparts[0] not in ("", "NULL") else None)
                    shape = (int(tparts[1])
                             if len(tparts) > 1 and tparts[1] not in ("", "NULL") else None)
                except ValueError:
                    raise ConfigError(
                        "inventory_replace: 模板 index=%d 的 StdMode/Shape 无法解析：%r"
                        % (op["std_index"], tpl[0][:200]))
                if op["std_mode"] is not None and std_mode != op["std_mode"]:
                    raise ConfigError(
                        "inventory_replace: index=%d 的 StdMode=%r 与声明 %r 不符"
                        % (op["std_index"], std_mode, op["std_mode"]))
                if op["ani_count"] is not None and shape != op["ani_count"]:
                    raise ConfigError(
                        "inventory_replace: index=%d 的 Shape=%r 与声明 ani_count=%r 不符"
                        % (op["std_index"], shape, op["ani_count"]))
                ev.update({"std_index": op["std_index"], "make_index": op["make_index"],
                           "dura": op["dura"], "dura_max": op["dura_max"]})
            else:
                if ev["before"] is None:
                    raise ConfigError(
                        "inventory_drop: Position=%d 本来为空——夹具没生效，拒绝"
                        % op["position"])
            res["targets"].append(ev)

        for op, ev in zip(ops, res["targets"]):
            pos = op["position"]
            write_sql = _inv_write_sql(db, pid, op)
            res["sql"]["write_%d" % pos] = write_sql.strip()
            sql("inventory_fixture.write_%d" % pos, write_sql)
            ev["applied"] = True
            got = _parse_inv_rows(sql("inventory_fixture.verify_%d" % pos,
                                      _inv_row_sql(db, pid, [pos])))
            ev["after"] = got.get(pos)
            if op["op"] == "replace":
                row = ev["after"]
                want = {"Position": pos, "MakeIndex": op["make_index"],
                        "StdIndex": op["std_index"], "Dura": op["dura"],
                        "DuraMax": op["dura_max"]}
                if row is None:
                    raise ConfigError("inventory_replace: Position=%d 写后读不到行" % pos)
                for k, v in want.items():
                    if row.get(k) != v:
                        raise ConfigError(
                            "inventory_replace: Position=%d 读回 %s=%r != %r"
                            % (pos, k, row.get(k), v))
            else:
                if ev["after"] is not None:
                    raise ConfigError("inventory_drop: Position=%d 写后仍有行：%r"
                                      % (pos, ev["after"]))
        after_fp = _parse_inventory_fp(sql("inventory_fixture.verify", fp_sql))
        res["fingerprint"]["after"] = after_fp
        b, a = res["fingerprint"]["before"], after_fp
        if a["count"] != b["count"]:
            raise ConfigError(
                "背包替换/丢弃: 其它槽位的行数变了（%d → %d）——动到了未声明的行"
                % (b["count"], a["count"]))
        if a["fp"] != b["fp"]:
            raise ConfigError("背包替换/丢弃: 其它槽位的指纹变了（%r → %r）"
                              % (b["fp"], a["fp"]))
        res["action"] = "applied"
        res["rc"] = 0
        return res
    except (ConfigError, OSError) as e:
        rb = _rollback_inventory_fixture(runner, res, env, timeout)
        res["rollback_error"] = rb
        res["error"] = "背包替换/丢弃夹具失败：%s" % e
        res["rc"] = 2
        return res


def _rollback_inventory_fixture(runner, res, env, timeout=120):
    """就地回滚已应用项（写后校验失败时用）；返回错误串或 None。"""
    sess = res.get("_session") or {}
    argv = sess.get("argv")
    db = res.get("db")
    pid = res.get("player_id")
    if not argv or not db or pid is None:
        return "背包夹具回滚缺少连接信息"
    errs = []
    for ev in reversed(res.get("targets") or []):
        if not ev.get("applied"):
            continue
        pos = ev["position"]
        try:
            text = _inv_restore_sql(db, pid, pos, ev.get("before"))
            rc, out, e = runner.run(res["side"], "inventory_fixture.rollback_%d" % pos,
                                    argv, env=env, timeout=timeout,
                                    stdin_text=text, capture=True)
            if rc != 0:
                errs.append("Position=%d 回滚 mysql rc=%s：%s"
                            % (pos, rc, (e or out or "").strip()[:200]))
                continue
            rc, out, e = runner.run(
                res["side"], "inventory_fixture.rollback_verify_%d" % pos, argv,
                env=env, timeout=timeout,
                stdin_text=_inv_row_sql(db, pid, [pos]), capture=True)
            if rc != 0:
                errs.append("Position=%d 回滚后读回失败（mysql rc=%s）" % (pos, rc))
                continue
            got = _parse_inv_rows(out)
            if got.get(pos) != ev.get("before"):
                errs.append("Position=%d 回滚后行 %r != 写前 %r"
                            % (pos, got.get(pos), ev.get("before")))
        except (ConfigError, OSError) as e:
            errs.append("Position=%d 回滚异常：%s" % (pos, e))
    return "；".join(errs) if errs else None


def restore_inventory_fixture(runner, res, env, timeout=120):
    """停栈后按快照整行还原，并**重读**逐槽校验；失败返回错误串（调用方升级 rc=2）。"""
    if not res or res.get("action") not in ("applied",):
        return None
    errs = []
    sess = res.get("_session") or {}
    argv = sess.get("argv")
    db, pid = res.get("db"), res.get("player_id")
    if not argv or not db or pid is None:
        return "背包夹具还原缺少连接信息"
    for ev in reversed(res.get("targets") or []):
        pos = ev["position"]
        try:
            text = _inv_restore_sql(db, pid, pos, ev.get("before"))
            rc, out, e = runner.run(res["side"], "inventory_fixture.restore_%d" % pos,
                                    argv, env=env, timeout=timeout,
                                    stdin_text=text, capture=True)
            if rc != 0:
                errs.append("Position=%d 还原 mysql rc=%s：%s"
                            % (pos, rc, (e or out or "").strip()[:200]))
                continue
            rc, out, e = runner.run(
                res["side"], "inventory_fixture.restore_verify_%d" % pos, argv,
                env=env, timeout=timeout,
                stdin_text=_inv_row_sql(db, pid, [pos]), capture=True)
            if rc != 0:
                errs.append("Position=%d 还原后读回失败（mysql rc=%s）" % (pos, rc))
                continue
            got = _parse_inv_rows(out)
            if got.get(pos) != ev.get("before"):
                errs.append("Position=%d 还原后行 %r != 写前 %r"
                            % (pos, got.get(pos), ev.get("before")))
        except (ConfigError, OSError) as e:
            errs.append("Position=%d 还原异常：%s" % (pos, e))
    return "；".join(errs) if errs else None


# ---------------------------------------------------------------------------
# 资产声明通道（assets）：restore 之后、起栈之前把「角色资产列」UPDATE 到目标值
#
# 卡 `openmir2-window-gold-declaration-channel`。部署前置卡
# （openmir2-window-deploy-prereq-batch1）披露：upgrade_retrieve 需要「Gold>=10000」
# 前置，而 placement（只写 MapName/CX/CY）与 inventory（只写 characters_bagitem）
# 两通道都不含 characters.Gold，金币前置无法声明。本通道让场景顶层可以声明
# 「characters 白名单资产列 = 目标值」（首版仅 Gold）。
#
# 选型与预置位/背包一致：DB 改**测试库**（restore 之后、起栈之前；角色记录只在
# 登录时读一次）；脚本/GM 路径不可行（登录后才跑 / Permission 恒 0）。
#
# 声明形态（场景顶层；BNF 见本卡报告）：
#   "assets": {"Gold": 10000, "note": "upgrade_retrieve 前置 Gold>=10000"}
# - **列白名单**（首版仅 `Gold`）：非白名单列 => 加载期 rc=2 点名（fail-closed）；
#   扩展只允许「有符号整数、语义是资产/点数、服务端只在登录/结算时读」的列
#   （实表取证：mir2_test_db.characters.Gold 是 int(11)，DESCRIBE 2026-10-24）；
# - 值必须是 JSON 整数（bool/浮点/字符串/null 一律拒）、0..2147483647（负数一律红）；
# - 每侧 UPDATE 该侧行（按 profile char_name/account 定位，ChrName+LoginID 唯一、
#   未删除），写前后对**其余列**做 MD5 指纹对比（只许写声明列），写后读回断言目标值；
# - `--dry-run` 打印预览行（含 列=目标值）并做离线校验（库名/库配对），不连库。
# 边界：probe 侧 `load_scenario` 的顶层键白名单暂未放行 `assets`——场景 JSON 采用
# 本通道时需在采卡 delta 里同步加一行 optional 键（见本卡报告「边界」节）。
# ---------------------------------------------------------------------------
#: 资产声明**只允许**写这些列；扩展规则：只加「有符号整数、语义是
#: 角色资产/点数、服务端只在登录/结算时读」的列，绝不加字符串列/时间列/主键外键列。
#: 2026-10-25（卡 openmir2-window-kit-p0-unblock）：`Gold` → 加 `GamePoint`。
#: `characters.GamePoint int(11) NULL DEFAULT 0`（`sql/mir2_db.sql:38`，注释「金刚石」；
#: Rust db-srv 读写见 `bins/db-srv/src/playdata.rs` 的 `game_point`）。
#: 用途：GAMEPOINT 真机窗口（`真爱-G002` `@yhua→@9hua` 前置 `CHECKGAMEPOINT > 8`）。
ASSETS_COLUMNS = ("Gold", "GamePoint")
#: 声明值上限：characters.Gold 是 int(11) 有符号（DESCRIBE 实表），负值一律拒。
ASSETS_MAX = 0x7FFFFFFF
#: 通道依赖的列（缺任何一个 => 表结构与预期不符， fail-closed）。
ASSETS_REQUIRED_COLUMNS = ("Id", "ChrName", "LoginID", "Deleted")
#: 声明里的备注键（不参与 UPDATE， 只进证据）。
ASSETS_NOTE_KEY = "note"


def scenario_assets(path):
    """读场景顶层 `assets` 声明（白名单列 → 目标值）。没有 => None；形状非法 => ConfigError。"""
    try:
        with open(path, "r", encoding="utf-8-sig") as f:
            data = json.load(f)
    except (OSError, ValueError) as e:
        raise ConfigError("%s: cannot load scenario: %s" % (path, e))
    if not isinstance(data, dict):
        raise ConfigError("%s: scenario must be a JSON object" % path)
    assets = data.get("assets")
    if assets is None:
        return None
    if not isinstance(assets, dict) or not assets:
        raise ConfigError("%s: scenario.assets 必须是非空对象（列→目标值）" % path)
    bad = set(assets) - set(ASSETS_COLUMNS) - {ASSETS_NOTE_KEY}
    if bad:
        raise ConfigError(
            "%s: scenario.assets 有不在白名单里的列 %s（fail-closed；白名单=%s，"
            "只允许声明白名单内的数字列）" % (path, sorted(bad), list(ASSETS_COLUMNS)))
    if assets.get(ASSETS_NOTE_KEY) is not None \
            and not isinstance(assets[ASSETS_NOTE_KEY], str):
        raise ConfigError("%s: scenario.assets.note 必须是字符串" % path)
    out = {}
    for col in ASSETS_COLUMNS:
        if col not in assets:
            continue
        v = assets[col]
        if not isinstance(v, int) or isinstance(v, bool):
            raise ConfigError(
                "%s: scenario.assets.%s 必须是 JSON 整数（实际 %r：%s）——"
                "字符串/浮点/bool/null 一律拒绝" % (path, col, v, type(v).__name__))
        if v < 0 or v > ASSETS_MAX:
            raise ConfigError(
                "%s: scenario.assets.%s=%r 越界（0..%d；characters.%s 是 int(11) "
                "有符号，负数/超 int32 一律拒绝）" % (path, col, v, ASSETS_MAX, col))
        out[col] = v
    if not out:
        raise ConfigError("%s: scenario.assets 只有 note、没有声明任何列（无意义，拒绝）"
                          % path)
    out[ASSETS_NOTE_KEY] = assets.get(ASSETS_NOTE_KEY) or ""
    return out


def _assets_row_sql(db, chr_name, login_id, columns, declared_cols):
    """读角色一行：命中行数 / Id / 各声明列现值 / 其余列 MD5 指纹 / Deleted。

    口径与 `_placement_row_sql` 同族：指纹只含**未声明列**（声明列允许变），
    `MD5(CONCAT_WS(CHAR(31), IFNULL(CAST(col AS CHAR), '~')))`——字符串 '~' 与 SQL
    NULL 在该口径下碰撞（与 dbsnap L2 同族限制；本判据要抓的是「非预期写入」）。
    characters 实表 42 列 vs 白名单个位数，`fp_cols` 为空在现网不会发生；若发生则退化为
    常量指纹（只比声明现值与行存在性），防御性兜底同 `inventory_fingerprint_sql`。
    """
    fp_cols = [c for c in columns if c not in declared_cols]
    sel = ", ".join("COALESCE(MIN(`%s`), -1)" % c for c in declared_cols) or "''"
    # 声明列必须**先在派生表里透传**（真库实锤 2026-10-24：外层 MIN 引用派生表没有的列
    # 会 1054 Unknown column）。
    inner = (", " + ", ".join("`%s`" % c for c in declared_cols)) if declared_cols else ""
    fp_inner = ("MD5(CONCAT_WS(%s))" % ", CHAR(31), ".join(
        "IFNULL(CAST(`%s` AS CHAR), '~')" % c for c in fp_cols)) if fp_cols else "''"
    return (
        "SELECT COUNT(*), COALESCE(MIN(`Id`), 0), %s,\n"
        "       COALESCE(MIN(`_fp`), ''), COALESCE(MIN(`_deleted`), 0)\n"
        "FROM (SELECT `Id`%s, IFNULL(`Deleted`, 0) AS `_deleted`, %s AS `_fp`\n"
        "      FROM `%s`.`characters` WHERE `ChrName` = %s AND `LoginID` = %s) AS `t`;\n"
        % (sel, inner, fp_inner, db, _sql_quote(chr_name), _sql_quote(login_id)))


def _parse_assets_row(out, n_decl):
    """解析 `_assets_row_sql` 的 n+4 列输出；形状不对 => ConfigError。"""
    lines = [ln for ln in (out or "").splitlines() if ln.strip()]
    if len(lines) != 1:
        raise ConfigError("资产读行结果不是 1 行（实际 %d 行）：%r"
                          % (len(lines), out[:300]))
    parts = lines[0].split("\t")
    if len(parts) != n_decl + 4:
        raise ConfigError("资产读行结果的列数不是 %d（实际 %d）：%r"
                          % (n_decl + 4, len(parts), lines[0][:300]))
    try:
        return {"count": int(parts[0]), "id": int(parts[1]),
                "values": [int(p) for p in parts[2:2 + n_decl]],
                "fp": parts[2 + n_decl], "deleted": int(parts[3 + n_decl] or 0)}
    except ValueError as e:
        raise ConfigError("资产读行结果无法解析：%r（%s）" % (lines[0][:300], e))


# ---------------------------------------------------------------------------
# 存档落定等待（卡 `openmir2-harness-save-time-symmetry`）
#
# 背景（实验 A，dev-1 comment `19f12a34`）：`run_side` 的 DB 快照在 probe 之后、停栈之前
# 无条件执行（对称），但**运行期断线存档的同步性不对称**——Rust 在 gate 事件里同步
# `save_human_rcd`（`lib.rs:22452`），C# 走 `WorldServer.cs:710-715` 离线支 → 异步
# FrontEngine→DBSrv→MySQL；harness 立刻拍库、随后 finally 强杀 C# ⇒ 「Rust 已落库、
# C# 仍是基线值」，DB 面被误读成行为差异（W1 `characters_ablity.Weight a=14/b=0`）。
#
# 本段在 **probe 之后、dbsnap 之前**插入可观测的「存档落定」等待：
#   * 判据是**静默检测**：轮询目标行（`characters.LASTUPDATE` + 行数 +
#     `characters_ablity.ModifyTime` 第二见证）指纹，连续两次
#     采样相同且时间跨度 >= `quiet_ms` 才认为落定；**不用固定 sleep 充当证据**；
#   * `require_changes=true` 时，未观测到变化的 quiet 不提前收口；持续采样到
#     `window_ms`，延迟变化会重新起算 quiet。观满窗口仍零变化才记 no_change_observed；
#   * 窗口 `window_ms`（默认 5000，上限 5000）内不落定 ⇒ **fail-closed**：该侧 rc=2 并记
#     `save_unsettled`（点名前后的采样），绝不回退成「等够了就当成功」；
#   * `--dry-run` 只打印计划、不连库。
#
# 证据：`<side>/save_settle.json` = {side,db,enabled,window_ms,quiet_ms,poll_ms,
# require_changes, samples:[{t_ms,sha256_12}], changes, settled, elapsed_ms, action, rc, error}。
SAVE_SETTLE_DEFAULTS = {"enabled": True, "window_ms": 5000, "quiet_ms": 800,
                        "poll_ms": 200, "require_changes": True}
SAVE_SETTLE_KEYS = ("enabled", "window_ms", "quiet_ms", "poll_ms", "require_changes")
SAVE_SETTLE_MAX_WINDOW_MS = 5000


def profile_save_settle(prof, side=None):
    """读 `save_settle` 配置（顶层默认，`sides.<side>.save_settle` 覆盖本侧）。

    形状非法 fail-closed：未知键、`enabled`/`require_changes` 非 bool、`window_ms` 越界
    （0..5000）、`quiet_ms` 越界（0..window_ms）、`poll_ms` 越界（50..1000）。

    `require_changes`（**默认 true**）：静默是**负向判据**——恒定指纹同样会被 `settle_decision`
    判成落定（`changes == 0` 也 settled，reviewer-21 实测）。所以默认要求**从 arm 点起至少
    观测到 1 次变化**，否则持续采样到 window 结束，该侧 DB 面不采信
    （rc=2 / `no_change_observed`）。**静默 != 存档已触发。** 确实不写库的只读场景才可显式 `require_changes: false`，
    并在该线程 `comment` 里写明「本场景不产生状态变化」。
    """
    decl = prof.get("save_settle")
    if side is not None:
        sc = (prof.get("sides") or {}).get(side) or {}
        if "save_settle" in sc:
            decl = sc["save_settle"]
    out = dict(SAVE_SETTLE_DEFAULTS)
    if decl is None:
        return out
    if not isinstance(decl, dict):
        raise ConfigError("save_settle 必须是对象（可被 sides.<side>.save_settle 覆盖）")
    bad = sorted(set(decl) - set(SAVE_SETTLE_KEYS))
    if bad:
        raise ConfigError("save_settle 有未知键 %s（只允许 %s）" % (bad, list(SAVE_SETTLE_KEYS)))
    for key in ("enabled", "require_changes"):
        if key not in decl:
            continue
        if not isinstance(decl[key], bool):
            raise ConfigError("save_settle.%s 必须是 bool（实际 %r）" % (key, decl[key]))
        out[key] = decl[key]
    for key, lo, hi in (("window_ms", 0, SAVE_SETTLE_MAX_WINDOW_MS),
                        ("quiet_ms", 0, SAVE_SETTLE_MAX_WINDOW_MS),
                        ("poll_ms", 50, 1000)):
        if key not in decl:
            continue
        v = decl[key]
        if not isinstance(v, int) or isinstance(v, bool):
            raise ConfigError("save_settle.%s 必须是整数（实际 %r）" % (key, v))
        if v < lo or v > hi:
            raise ConfigError("save_settle.%s=%r 越界（允许 %d..%d）" % (key, v, lo, hi))
        out[key] = v
    if out["quiet_ms"] > out["window_ms"]:
        raise ConfigError("save_settle.quiet_ms=%d > window_ms=%d（永远不可能落定）"
                          % (out["quiet_ms"], out["window_ms"]))
    return out


def summary_save_settle_decl(prof):
    """summary 用：合法则给出每侧生效的 `save_settle` 配置，非法则 None。

    真正的 fail-closed 在 `run_side`（`profile_save_settle` 抛 ConfigError ⇒ 该侧 rc=2）；
    这里只是为了在报告里如实展示声明，**不吞掉**那边的报错。
    """
    try:
        return dict((side, profile_save_settle(prof, side)) for side in SIDES)
    except ConfigError:
        return None


def settle_decision(samples, quiet_ms):
    """静默检测：`samples` = [(t_ms, fingerprint)] 按时间升序。

    返回 (settled, changes, elapsed_ms)。**落定 = 当前末尾连续段在 quiet_ms 内没有变化**；
    新变化会撤销此前已达成的 quiet，固定 sleep 不参与判定。
    """
    if not samples:
        return False, 0, 0
    changes = 0
    for i in range(1, len(samples)):
        if samples[i][1] != samples[i - 1][1]:
            changes += 1
    # 落定 = **当前这段「指纹不变」的连续区间**已持续 >= quiet_ms（不是相邻采样间隔）。
    settled = False
    run_start = samples[0][0]
    for i in range(1, len(samples)):
        if samples[i][1] != samples[i - 1][1]:
            run_start = samples[i][0]
            settled = False
        elif (samples[i][0] - run_start) >= quiet_ms:
            settled = True
    return settled, changes, samples[-1][0] - samples[0][0]


def _save_settle_sql(db, chr_name):
    """目标行指纹：`characters.LASTUPDATE` + 行数 + `characters_ablity.ModifyTime`。

    两列都是**两侧存档都会刷新**的时间戳（C# `PlayDataStorage.Save.cs:85-88`、
    Rust `playdata.rs:423-427`；ablitity 侧 `:141` / `:486`，逐字同形），所以给
    `LASTUPDATE` 配一个第二见证：任一列变了，指纹就变。

    列名以**冻结 schema** 为准（`sql/mir2_db.sql`）：`characters` 的时间列是 `LASTUPDATE`，
    **没有** `ModifyTime`（那在 `characters_ablity` / `account` 上）——写错会让每侧 mysql 报
    Unknown column、两侧都 rc=2（review `b48197f6` F1）。离线牙 `SETTLE13/13b/14` 把 SQL 里
    出现的标识符逐个对 `sql/mir2_db.sql` 解析出的列集校验。
    """
    return ("/* save_settle */ SELECT COALESCE(MAX(c.`LASTUPDATE`), ''), "
            "COUNT(DISTINCT c.`Id`), COALESCE(MAX(a.`ModifyTime`), '') "
            "FROM `%s`.`characters` AS c "
            "LEFT JOIN `%s`.`characters_ablity` AS a ON a.`PlayerId` = c.`Id` "
            "WHERE c.`ChrName` = %s;\n"
            % (db, db, _sql_quote(chr_name)))


def _save_settle_witness(fingerprint):
    """把一次采样的原始返回值拆成见证三元组 `[lastupdate, count, abil_modifytime]`。

    返回 (witness, error)。`mysql --batch --skip-column-names` 给的是**一行制表符分隔**的
    三列（见 `_save_settle_sql`）。解析失败**不改变判据**（判据仍是整行指纹的 sha256），
    只在样本里留 `witness_error`。留原始值是给真机轮次看的：变化来自哪一列、基线是不是
    NULL、两次变化是否落在同一秒（`LASTUPDATE` 是 `datetime`，秒粒度）。
    """
    parts = (fingerprint or "").strip("\r\n").split("\t")
    if len(parts) != 3:
        return None, "列数 %d != 3（原始 %r）" % (len(parts), (fingerprint or "")[:120])
    return parts, None


def _schema_columns(table, sql_path=None):
    """从**冻结 schema** `sql/mir2_db.sql` 解析 `table` 的列名集合（离线，不连库）。

    只认 `CREATE TABLE `<t>` (` 之后到首个顶格 `)` 之间的、行首缩进后的第一个反引号标识符
    ——足够挡住「把不存在的列写进指纹 SQL」这类错误（review `b48197f6` F1：`characters` 上
    没有 `ModifyTime`），不假装是完整 SQL 解析器。离线牙 `SETTLE13/13b/14` 用它。
    """
    path = sql_path or os.path.join(REPO_ROOT, "sql", "mir2_db.sql")
    cols = set()
    inside = False
    with io.open(path, encoding="utf-8") as f:
        for line in f:
            if not inside:
                if re.match(r"CREATE TABLE `%s`\s*\(" % re.escape(table), line):
                    inside = True
                continue
            if line.startswith(")"):
                break
            m = re.match(r"\s+`([^`]+)`", line)
            if m:
                cols.add(m.group(1))
    return cols


def _sql_identifiers(sql):
    """取出 SQL 里的反引号标识符（先剥掉块注释，`/* save_settle */` 不是标识符）。"""
    body = sql.split("*/", 1)[-1]
    return set(re.findall(r"`([^`]+)`", body))


def wait_save_settled(runner, side, prof, sc, settle, env, sql_fn=None,
                      timeout=120, clock=None):
    """probe 之后、dbsnap 之前的「存档落定」等待（卡 `openmir2-harness-save-time-symmetry`）。

    `sql_fn(sql_text) -> str` 可用测试夹具注入；生产路径走该侧 mysql 客户端。
    失败（DB 不可用 / 窗口内不落定）⇒ rc=2 + `save_unsettled`，调用方不得继续拍库。
    """
    import time as _time

    def _now_ms():
        return clock() if clock is not None else int(_time.monotonic() * 1000)

    def _sleep(ms):
        if clock is None:
            _time.sleep(max(0, ms) / 1000.0)

    res = {"side": side, "db": None, "enabled": bool(settle.get("enabled")),
           "window_ms": settle.get("window_ms"), "quiet_ms": settle.get("quiet_ms"),
           "poll_ms": settle.get("poll_ms"),
           # 缺键必须落到**安全侧**（True）：不经过 profile_save_settle() 的调用（测试、
           # 将来新加的路径）若默认 False，就会把「静默是负向判据」这道门静默关掉
           # （reviewer-21 delta 实测：不带该键 + 恒定指纹 => rc=0 settled / changes=0）。
           "require_changes": bool(settle.get("require_changes", True)),
           "samples": [], "changes": 0,
           "witness_first": None, "witness_last": None,
           "settled": False, "elapsed_ms": 0, "action": None, "rc": None,
           "error": None}
    if not res["enabled"]:
        res["action"] = "disabled"
        res["rc"] = 0
        return res
    chr_name = str(prof.get("char_name") or "")
    try:
        db = placement_characters_db(side, sc)
        res["db"] = db
        if not chr_name:
            raise ConfigError("存档落定等待需要 profile 的 char_name（定位 characters 行）")
        if sql_fn is None:
            client = placement_db_client(prof, side)
            argv = _mysql_argv(client)

            def sql_fn(text):
                rc, out, err = runner.run(side, "save_settle", argv, env=env,
                                          timeout=timeout, stdin_text=text, capture=True)
                if rc != 0:
                    raise ConfigError("save_settle 查询失败（mysql rc=%s）：%s"
                                      % (rc, (err or out or "").strip()[:300]))
                return out
    except ConfigError as e:
        res["error"] = "存档落定等待前置校验失败：%s" % e
        res["rc"] = 2
        return res
    if getattr(runner, "dry_run", False):
        res["action"] = "dry-run"
        runner.log("[%s] 存档落定等待（dry-run 不连库）：db=%s 行=%s window=%dms quiet=%dms "
                   "poll=%dms" % (side, db, chr_name, res["window_ms"], res["quiet_ms"],
                                  res["poll_ms"]))
        res["rc"] = 0
        return res
    sql = _save_settle_sql(db, chr_name)
    t_start = _now_ms()
    try:
        while True:
            t = _now_ms()
            # 只去行尾换行：\t1\tts（LASTUPDATE 为 NULL）与 ts\t1\t（无 ablity 行）里的
            # 空列必须留着——那正是本 residual 要诊断的形态；.strip() 会把它们吃成 2 列。
            fp = sql_fn(sql).strip("\r\n")
            witness, werr = _save_settle_witness(fp)
            sample = {"t_ms": t - t_start,
                      "sha256_12": hashlib.sha256(fp.encode("utf-8")).hexdigest()[:12],
                      "witness": witness}
            if werr:
                sample["witness_error"] = werr
            res["samples"].append(sample)
            if res.get("witness_first") is None:
                res["witness_first"] = witness
            res["witness_last"] = witness
            settled, changes, _ = settle_decision(
                [(s["t_ms"], s["sha256_12"]) for s in res["samples"]], res["quiet_ms"])
            res["changes"] = changes
            elapsed = t - t_start
            res["settled"] = settled
            res["elapsed_ms"] = elapsed
            # quiet 只决定「观测到变化后何时收口」；require_changes=true 且尚未变化时
            # 不能把静默当成失败提前返回，否则异步落库晚于 quiet 就永远观测不到。
            if settled and (not res["require_changes"] or res["changes"] > 0):
                res["action"] = "settled"
                res["rc"] = 0
                runner.log("[%s] 存档落定：db=%s 行=%s changes=%d elapsed=%dms samples=%d"
                           % (side, db, chr_name, changes, elapsed, len(res["samples"])))
                return res
            if elapsed >= res["window_ms"]:
                break
            _sleep(min(res["poll_ms"], max(0, res["window_ms"] - elapsed)))
        res["elapsed_ms"] = _now_ms() - t_start
        if res["require_changes"] and res["changes"] == 0:
            # 观满整个 window 仍无变化才是 no_change；此时静默仍不等于存档已触发。
            res["action"] = "no_change_observed"
            res["error"] = ("save_unsettled: no_change_observed —— require_changes=true "
                            "但实际观测的 %dms 内没有任何变化（changes=0，采样 %d 次；"
                            "db=%s 行=%s；window 上限 %dms）—— 静默 != 存档已触发，"
                            "该侧 DB 面不采信"
                            % (res["elapsed_ms"], len(res["samples"]), db, chr_name,
                               res["window_ms"]))
            res["rc"] = 2
            return res
        res["action"] = "timeout"
        res["error"] = ("save_unsettled: %dms 窗口内未观测到 %dms 静默（changes=%d，采样 %d 次；"
                        "db=%s 行=%s）—— 存档可能仍在途或未触发，拒绝把本次 DB 快照当证据"
                        % (res["window_ms"], res["quiet_ms"], res["changes"],
                           len(res["samples"]), db, chr_name))
        res["rc"] = 2
        return res
    except ConfigError as e:
        res["error"] = "save_unsettled: %s" % e
        res["rc"] = 2
        return res


def run_assets(runner, side, prof, sc, assets, env, timeout=120):
    """在一侧执行资产声明通道：restore 之后、起栈之前。返回证据 dict（rc=0 才算完成）。

    fail-closed：目标角色不存在/多行/已删除、缺列、UPDATE 影响行数 != 1、写后读回与
    目标不符、其余列指纹变化 => rc=2 点名，绝不带着错位继续 probe。
    """
    declared = dict((k, v) for k, v in assets.items() if k != ASSETS_NOTE_KEY)
    res = {"side": side, "declared": dict(assets), "source": "scenario", "db": None,
           "db_pairing": None, "player_id": None, "action": None, "before": None,
           "after": None, "changed_columns": [], "sql": {}, "rc": None, "error": None}
    if not declared:
        res["action"] = "noop"
        res["rc"] = 0
        return res
    try:
        db = placement_characters_db(side, sc)
        res["db"] = db
        res["db_pairing"] = _check_db_pairing(side, placement_envir_root(side, sc), db)
        client = placement_db_client(prof, side)
    except ConfigError as e:
        res["error"] = "资产声明通道前置校验失败：%s" % e
        res["rc"] = 2
        return res
    argv = _mysql_argv(client)
    chr_name = str(prof.get("char_name") or "")
    login_id = str(prof.get("account") or "")
    if not chr_name or not login_id:
        res["error"] = ("资产声明通道需要 profile 的 char_name/account"
                        "（定位 characters 行）；当前 char_name=%r account=%r"
                        % (chr_name, login_id))
        res["rc"] = 2
        return res
    cols_sorted = sorted(declared)
    if getattr(runner, "dry_run", False):
        pairs = ", ".join("%s=%d" % (c, declared[c]) for c in cols_sorted)
        res["action"] = "dry-run"
        res["sql"] = {"declare": "UPDATE `%s`.`characters` SET %s WHERE Id=<row>"
                                 "（声明 %d 列）" % (db, pairs, len(declared))}
        runner.log("[%s] 资产声明通道（dry-run 不连库）：db=%s %s 角色=%s"
                   % (side, db, pairs, chr_name))
        res["rc"] = 0
        return res

    def sql(tag, text):
        rc, out, err = runner.run(side, tag, argv, env=env, timeout=timeout,
                                  stdin_text=text, capture=True)
        if rc != 0:
            raise ConfigError("%s 执行失败（mysql rc=%s）：%s"
                              % (tag, rc, (err or out or "").strip()[:400]))
        return out

    try:
        cols_out = sql("assets.columns",
                       "SHOW COLUMNS FROM `%s`.`characters`;\n" % db)
        columns = [ln.split("\t")[0] for ln in cols_out.splitlines() if ln.strip()]
        missing = [c for c in ASSETS_REQUIRED_COLUMNS + tuple(cols_sorted)
                   if c not in columns]
        if missing:
            raise ConfigError(
                "%s.characters 缺列 %s：表结构与预期不符（资产通道不改表结构，拒绝）"
                % (db, missing))
        read_sql = _assets_row_sql(db, chr_name, login_id, columns, cols_sorted)
        res["sql"]["read"] = read_sql.strip()
        before = _parse_assets_row(sql("assets.read", read_sql), len(cols_sorted))
        res["player_id"] = before["id"]
        res["before"] = {"id": before["id"],
                         "values": dict(zip(cols_sorted, before["values"])),
                         "fingerprint": before["fp"]}
        if before["count"] == 0:
            raise ConfigError(
                "资产声明通道找不到角色：%s.characters WHERE ChrName=%r AND LoginID=%r "
                "命中 0 行" % (db, chr_name, login_id))
        if before["count"] > 1:
            raise ConfigError(
                "资产声明通道定位不唯一：%s.characters 命中 %d 行（ChrName=%r "
                "LoginID=%r）——拒绝猜哪一行"
                % (db, before["count"], chr_name, login_id))
        if before["deleted"] == 1:
            raise ConfigError("角色 %r 的 Deleted=1（已删除）：资产通道拒绝改已删角色"
                              % chr_name)
        if before["values"] == [declared[c] for c in cols_sorted]:
            res["action"] = "noop"
        else:
            sets = ", ".join("`%s` = %d" % (c, declared[c]) for c in cols_sorted)
            upd = ("UPDATE `%s`.`characters` SET %s\n"
                   " WHERE `Id` = %d;\nSELECT ROW_COUNT();\n"
                   % (db, sets, before["id"]))
            res["sql"]["update"] = upd.strip()
            out = sql("assets.update", upd).strip()
            try:
                affected = int((out.splitlines() or ["-1"])[0])
            except ValueError:
                raise ConfigError("assets.update 的 ROW_COUNT 不可解析：%r" % out[:200])
            if affected != 1:
                raise ConfigError(
                    "assets.update 影响了 %d 行（期望 1）：角色行可能在预置期间被删/改，"
                    "拒绝继续" % affected)
            res["action"] = "updated"
        after = _parse_assets_row(sql("assets.verify", read_sql), len(cols_sorted))
        res["after"] = {"id": after["id"],
                        "values": dict(zip(cols_sorted, after["values"])),
                        "fingerprint": after["fp"]}
        if after["count"] != 1 or after["id"] != before["id"]:
            raise ConfigError(
                "资产声明通道读回异常：命中 %d 行、Id=%s（期望 1 行 Id=%s）"
                % (after["count"], after["id"], before["id"]))
        if after["values"] != [declared[c] for c in cols_sorted]:
            raise ConfigError(
                "资产声明通道读回与目标不符：期望 %r，实际 %r（列序 %s）"
                % ([declared[c] for c in cols_sorted], after["values"], cols_sorted))
        if after["fp"] != before["fp"]:
            raise ConfigError(
                "资产声明通道连带改了**其它列**（声明列之外的 MD5 指纹变了：%s -> %s）："
                "只允许写声明的 %r，拒绝带着被写坏的字段继续"
                % (before["fp"][:12] or "<空>", after["fp"][:12] or "<空>", cols_sorted))
        res["changed_columns"] = cols_sorted if res["action"] == "updated" else []
        res["rc"] = 0
        return res
    except ConfigError as e:
        res["error"] = "资产声明通道失败：%s" % e
        res["rc"] = 2
        return res


def preview_assets(runner, side, prof, sc, assets, ctx):
    """`--dry-run` 用：资产声明通道的离线校验 + 命令预览（不连库/不改库）。

    与 `preview_inventory` 同口径：只做离线校验（库名/库配对/客户端参数），
    前置不成立 => 返回 failed=True（真跑会 rc=2 并点名）。预览行带「列=目标值」。
    """
    out = {"side": side, "declared": dict(assets), "rc": None, "error": None,
           "commands": [], "offline_check": None}
    declared = dict((k, v) for k, v in assets.items() if k != ASSETS_NOTE_KEY)
    pairs = ", ".join("%s=%d" % (c, declared[c]) for c in sorted(declared))
    try:
        envir_root = placement_envir_root(side, sc)
        db = placement_characters_db(side, sc)
        pairing = _check_db_pairing(side, envir_root, db)
        client = placement_db_client(prof, side)
    except ConfigError as e:
        out["error"] = "资产声明通道前置校验失败（真跑会 rc=2）：%s" % e
        out["rc"] = 2
        return out, True
    chr_name = str(prof.get("char_name") or "")
    login_id = str(prof.get("account") or "")
    argv = _redact_argv(_mysql_argv(client))
    out["offline_check"] = {"envir_root": envir_root, "db": db,
                            "db_pairing": pairing, "char_name": chr_name}
    out["commands"] = [
        {"argv": argv, "stdin": "SHOW COLUMNS FROM `%s`.`characters`;" % db},
        {"argv": argv,
         "stdin": "SELECT COUNT(*), MIN(`Id`), <声明列现值>, <MD5 指纹>, Deleted FROM "
                  "`%s`.`characters` WHERE ChrName=%r AND LoginID=%r;"
                  % (db, chr_name, login_id)},
        {"argv": argv,
         "stdin": "UPDATE `%s`.`characters` SET %s WHERE `Id`=<row>;" % (db, pairs)},
        {"argv": argv, "stdin": "SELECT ...（读回校验：目标值 + 其余列 MD5 指纹）"},
    ]
    runner.log("[%s] 资产声明通道（dry-run 不连库）：db=%s %s 角色=%s"
               % (side, db, pairs, chr_name))
    out["rc"] = 0
    return out, False


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




def rng_env(side_dir):
    """一侧的 RNG 流水环境：路径 + 让 C# 钩子写 5 列（含 site）的开关。

    C# `rng-hook/StartupHook.cs:210` 只在 `MIR2_RNG_SITE=1` 时写 5 列
    （seq/kind/arg/result/site）；否则 `:399-406` 写 4 列 ⇒ `check_rng_ledger` 的
    「每行 5 列且 site 非空」必红。Rust 侧不读 `MIR2_RNG_SITE`（无害）。
    """
    return {
        "MIR2_RNG_LOG": os.path.join(side_dir, "rng-stream.tsv"),
        "MIR2_RNG_SITE": "1",
    }


def check_rng_ledger(path):
    """校验一侧 RNG 流水账本（`--rng-log`）：存在、非空、头两行自证、每行 5 列且 site 非空。

    格式（`mir2-rs/crates/shared/src/rng_trace.rs` 与 C# `rng-hook/StartupHook.cs` 同形）：
    `seq \t kind \t max/arg \t result \t site`，头两行 `INJECT_OK` 与 `SEED_PROBE`。
    返回 (ok, detail)；不 ok 时 detail 点名缺什么（供 rc=2 的 error）。
    """
    if not os.path.exists(path):
        return False, "RNG 账本不存在：%s（-RngLog 未贯通或进程未读 MIR2_RNG_LOG）" % path
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            lines = [ln.rstrip("\r\n") for ln in f if ln.strip()]
    except OSError as e:
        return False, "RNG 账本读失败：%s（%s）" % (path, e)
    if len(lines) < 2:
        return False, "RNG 账本为空/不足两行：%s（%d 行）" % (path, len(lines))
    head = "\n".join(lines[:4])
    if "INJECT_OK" not in head:
        return False, "RNG 账本缺 INJECT_OK 自证行：%s" % path
    if "SEED_PROBE" not in head:
        return False, "RNG 账本缺 SEED_PROBE 自证行：%s" % path
    for i, ln in enumerate(lines[2:], start=3):
        cols = ln.split("\t")
        if len(cols) < 5:
            return False, ("RNG 账本第 %d 行列数 %d < 5（seq/kind/max/result/site）：%r"
                           % (i, len(cols), ln[:160]))
        if not cols[4].strip():
            return False, "RNG 账本第 %d 行 site 为空：%r" % (i, ln[:160])
    return True, "%d 行（含 INJECT_OK + SEED_PROBE；site 齐）" % len(lines)


#: `profile.fixtures` 顶层允许的键。**两份能力合并**：
#:   * `monsters` —— 声明式怪物夹具（卡 `openmir2-window-fixture-corpse`）；
#:   * `config_overrides` / `castle_list` / `castle_files` —— 配置文件与城堡前置
#:     （卡 `openmir2-window-fixture-castle`）。
FIXTURE_KEYS = ("monsters", "config_overrides", "castle_list", "castle_files", "level",
                "inventory_replace", "inventory_drop", "files", "note")

#: 卡 `openmir2-gm-script-fixture` A5：`fixtures.files` 的白名单根 —— 只许写
#: `<envir_root>/Envir/` 下的相对路径（脚本注入面），禁绝对路径与 `..`。
FIXTURE_FILES_ROOT = "Envir"

#: `fixtures.inventory_replace` 条目的键白名单（槽位 → 物品）。
FIXTURE_INV_REPLACE_KEYS = {"position", "index", "make_index", "std_mode",
                            "ani_count", "dura", "dura_max", "note"}


def _validate_fixture_inv_replace(value, where):
    if not isinstance(value, list) or not value:
        raise ConfigError("%s.inventory_replace 必须是非空数组" % where)
    out, seen = [], set()
    for i, it in enumerate(value):
        w = "%s.inventory_replace[%d]" % (where, i)
        if not isinstance(it, dict):
            raise ConfigError("%s 必须是对象" % w)
        bad = set(it) - FIXTURE_INV_REPLACE_KEYS
        if bad:
            raise ConfigError("%s 有未知键 %s（allow=%s）"
                              % (w, sorted(bad), sorted(FIXTURE_INV_REPLACE_KEYS)))
        for k in ("position", "index", "make_index"):
            if k not in it:
                raise ConfigError("%s 缺必填键 %s" % (w, k))
        vals = {}
        for k in ("position", "index", "make_index", "std_mode", "ani_count",
                  "dura", "dura_max"):
            if k not in it or it[k] is None:
                continue
            v = it[k]
            if not isinstance(v, int) or isinstance(v, bool):
                raise ConfigError("%s.%s 必须是整数（实际 %r）" % (w, k, v))
            if v < 0 or v > 0xFFFFFFFF:
                raise ConfigError("%s.%s=%r 越界（0..4294967295）" % (w, k, v))
            vals[k] = v
        pos = vals["position"]
        if pos >= INVENTORY_MAX_BAG:
            raise ConfigError("%s.position=%d 越界（0..%d）"
                              % (w, pos, INVENTORY_MAX_BAG - 1))
        if pos in seen:
            raise ConfigError("%s.position=%d 重复（同一槽位只能声明一次）" % (w, pos))
        seen.add(pos)
        if vals["make_index"] <= 0:
            raise ConfigError("%s.make_index 必须 >0" % w)
        if it.get("note") is not None and not isinstance(it["note"], str):
            raise ConfigError("%s.note 必须是字符串" % w)
        out.append({"position": pos, "index": vals["index"],
                    "make_index": vals["make_index"],
                    "std_mode": vals.get("std_mode"),
                    "ani_count": vals.get("ani_count"),
                    "dura": vals.get("dura", 0), "dura_max": vals.get("dura_max", 0),
                    "note": it.get("note") or ""})
    return out


def _validate_fixture_inv_drop(value, where):
    if not isinstance(value, list) or not value:
        raise ConfigError("%s.inventory_drop 必须是非空数组" % where)
    out, seen = [], set()
    for i, it in enumerate(value):
        w = "%s.inventory_drop[%d]" % (where, i)
        if isinstance(it, dict):
            if set(it) - {"position", "note"} or "position" not in it:
                raise ConfigError("%s 只允许 {position, note}" % w)
            pos = it["position"]
            note = it.get("note") or ""
        else:
            pos, note = it, ""
        if not isinstance(pos, int) or isinstance(pos, bool):
            raise ConfigError("%s position 必须是整数（实际 %r）" % (w, pos))
        if pos < 0 or pos >= INVENTORY_MAX_BAG:
            raise ConfigError("%s.position=%d 越界（0..%d）"
                              % (w, pos, INVENTORY_MAX_BAG - 1))
        if pos in seen:
            raise ConfigError("%s.position=%d 重复" % (w, pos))
        seen.add(pos)
        out.append({"position": pos, "note": note})
    return out

#: 夹具行的第 7 列 `ZenTime`（分钟）**必须 > 0**：两端解析器都按 `ZenTime = 列值*60*1000` 后
#: `ZenTime <= 0 => 丢弃`（C# `LocalDB.cs:555-558`；Rust `crates/envir/src/mongen.rs:147-150`），
#: 且**加载期无日志** => 写 0 会造成「夹具看似应用、怪根本不刷」。首刷不受该值影响：
#: `WorldServer.MonGen.cs:228` 在 `StartTick == 0` 时立即首刷。
FIXTURE_MONGEN_ZEN_TIME = 360

#: 怪物夹具行的落盘位置。**必须是 prepend**（卡 `openmir2-fixture-monsters-prepend`）：
#: 刷怪线程按 `RegenMonstersTime=200ms` **每次只前进一条** MonGen
#: （C# `WorldServer.MonGen.cs:205-230` 取 `mongenList[CurrMonGenIdx]` 后 `CurrMonGenIdx++`；
#: Rust `crates/world/src/mongen.rs` 同构）⇒ 追加到单表 3081 行末尾 ≈ 616s（≈10 分钟）才轮得到，
#: 窗口内**永远不刷**。前插 = index 0，第一个闸门（约 200ms、`StartTick==0`）就取到。
FIXTURE_MONGEN_LAYOUT = "prepend"


def _mongen_write_layout(path, body, block):
    """把夹具块写到文件**头部**（prepend）。**测试变异点**：改成 `body + block`
    即 append 实现，用来复现 review F1（机判失败后必须按备份回滚）。"""
    with open(path, "wb") as f:
        f.write(block + body)


def _mongen_fixture_layout(raw_before, raw_after, lines):
    """机判夹具行是否在**头部**、且原内容字节整体后移未变。

    返回 `(ok, why)`。`why` 在失败时点名形态（append / 头部缺失 / 原内容被改），
    供 selftest 的「改回 append 必须红」牙齿使用（卡 `openmir2-fixture-monsters-prepend` ②）。
    """
    nl = b"\r\n" if b"\r\n" in raw_before else b"\n"
    body = raw_before if (not raw_before or raw_before.endswith(b"\n")) else raw_before + nl
    block = b"".join(ln.encode("gbk") + nl for ln in lines)
    if raw_after == body:
        return False, "夹具行没有写进文件"
    if raw_after.endswith(block) and raw_after[:-len(block)] == body:
        n_lines = raw_before.count(b"\n")
        if raw_before and not raw_before.endswith(b"\n"):
            n_lines += 1
        secs = n_lines * 0.2
        return False, ("夹具行位于文件末尾（append 布局）：刷怪线程每 200ms 只前进一条，"
                       "本文件 %d 行扫一轮 ≈ %.0fs（≈%.1f 分钟），窗口内轮不到"
                       % (n_lines, secs, secs / 60.0))
    if not raw_after.startswith(block):
        return False, "夹具行不在文件头部"
    if raw_after[len(block):] != body:
        return False, "前插后原文件内容字节不一致（不得改动原有行）"
    return True, "prepend：%d 行位于文件头，原内容字节整体后移" % len(lines)


#: RNG 台账（`MIR2_RNG_LOG`）体积上限：超过就**跑后**裁剪（只留 head + 统计）。
#: 背景（卡 `openmir2-fixture-monsters-prepend` ③）：C# 钩子每次抽数都落盘，
#: 20 分钟实测 899MB ⇒ 不裁剪会写满磁盘/拖垮传输。默认 64MiB / 20 万行。
RNG_LEDGER_MAX_BYTES = 64 * 1024 * 1024
RNG_LEDGER_HEAD_LINES = 200000


def rng_ledger_trim_allowed(stack_attempted, stop_rc):
    """只有**确认本侧栈已停**才允许裁剪台账（review F2：栈还在写时 os.replace 会丢/竞态）。"""
    return stop_ok(stack_attempted, stop_rc)


def trim_rng_ledger(path, max_bytes=None, head_lines=None):
    """台账超过上限 ⇒ 记录原始 size/行数/sha256，只保留前 `head_lines` 行。

    返回证据 dict（未超限时 `trimmed=False` 并给出 size/sha256/行数）。
    先写 `.head` 临时文件再 `os.replace`，避免半截文件。head 里保留
    `INJECT_OK`/`SEED_PROBE`（都在最前面），裁剪后 `check_rng_ledger` 仍可复核。
    """
    max_bytes = RNG_LEDGER_MAX_BYTES if max_bytes is None else int(max_bytes)
    head_lines = RNG_LEDGER_HEAD_LINES if head_lines is None else int(head_lines)
    stats = {"path": path, "trimmed": False, "original_bytes": 0,
             "original_lines": 0, "original_sha256": None,
             "kept_lines": 0, "kept_bytes": 0, "error": None}
    try:
        size = os.path.getsize(path)
    except OSError as e:
        stats["error"] = "台账不存在/不可读：%s" % e
        return stats
    stats["original_bytes"] = size
    h = hashlib.sha256()
    lines = 0
    try:
        with open(path, "rb") as f:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                h.update(chunk)
                lines += chunk.count(b"\n")
    except OSError as e:
        stats["error"] = "读台账失败：%s" % e
        return stats
    stats["original_lines"] = lines
    stats["original_sha256"] = h.hexdigest()
    if size <= max_bytes:
        stats["kept_lines"] = lines
        stats["kept_bytes"] = size
        return stats
    tmp = path + ".head"
    kept = 0
    kept_bytes = 0
    try:
        with open(path, "rb") as src, open(tmp, "wb") as dst:
            for ln in src:
                if kept >= head_lines:
                    break
                dst.write(ln)
                kept += 1
                kept_bytes += len(ln)
        os.replace(tmp, path)
    except OSError as e:
        stats["error"] = "裁剪台账失败：%s" % e
        try:
            if os.path.exists(tmp):
                os.remove(tmp)
        except OSError:
            pass
        return stats
    stats.update({"trimmed": True, "kept_lines": kept, "kept_bytes": kept_bytes})
    return stats

#: 配置覆盖白名单：键 → (lo, hi)。**未知文件/键/越界值一律 fail-closed**。
FIXTURE_CONFIG_KEYS = {
    "CastleGetAllNpcTax": (0, 1),
    "CastleTaxRate": (0, 100),
    "CastleOneDayGold": (0, 0x7FFFFFFF),
    "CastleGoldMax": (0, 0x7FFFFFFF),
    "UpgradeWeaponPrice": (0, 0x7FFFFFFF),
    "ConsoleShowUserCountTime": (0, 0x7FFFFFFF),
}
#: 允许覆盖的配置文件（都在 `<envir_root>/` 下）。
FIXTURE_CONFIG_FILES = ("Server.conf", "Server1.conf")
#: `Castle/<n>/SabukW.txt` 的 `[Setup]` 键白名单：键 → 值类型。
FIXTURE_CASTLE_KEYS = {
    "OwnGuild": "str",
    "NextWar": "int",
    "TotalGold": "int",
    "TodayIncome": "int",
}
FIXTURE_CASTLE_MAX = 8
#: 生产内容根：窗口夹具只许写私有根（写到这里 = 改冻结内容，直接拒绝）。
FIXTURE_PROD_ROOT = "E:/MirServer"
#: 单个怪物夹具条目的字段白名单。
FIXTURE_MONSTER_ALLOWED = {"map", "x", "y", "name", "count"}


def _validate_monsters(mons, where):
    """怪物夹具 schema（原卡 `openmir2-window-fixture-corpse`，语义原样保留）。"""
    if not isinstance(mons, list) or not mons:
        raise ConfigError("%s 必须是非空数组" % where)
    out = []
    for i, m in enumerate(mons):
        w = "%s[%d]" % (where, i)
        if not isinstance(m, dict):
            raise ConfigError("%s 必须是对象" % w)
        unknown = set(m) - FIXTURE_MONSTER_ALLOWED
        if unknown:
            raise ConfigError("%s 有未知字段 %s（只允许 %s）"
                              % (w, sorted(unknown), sorted(FIXTURE_MONSTER_ALLOWED)))
        for k in ("map", "x", "y", "name"):
            if k not in m:
                raise ConfigError("%s 缺字段 %s" % (w, k))
        mp = m["map"]
        if not isinstance(mp, str) or not mp.strip():
            raise ConfigError("%s.map 必须是非空字符串" % w)
        for k in ("x", "y"):
            v = m[k]
            if not isinstance(v, int) or isinstance(v, bool) or not (0 <= v <= 65535):
                raise ConfigError("%s.%s 必须是 0..65535 的整数（实际 %r）" % (w, k, v))
        name = m["name"]
        if not isinstance(name, str) or not name.strip():
            raise ConfigError("%s.name 必须是非空字符串" % w)
        try:
            name.encode("gbk")
        except UnicodeEncodeError:
            raise ConfigError("%s.name 不能编码为 GBK：%r" % (w, name))
        count = m.get("count", 1)
        if not isinstance(count, int) or isinstance(count, bool) or not (1 <= count <= 100):
            raise ConfigError("%s.count 必须是 1..100 的整数（实际 %r）" % (w, count))
        out.append({"map": mp.strip(), "x": m["x"], "y": m["y"],
                    "name": name, "count": count})
    return out


def _fixture_int(value, where, lo, hi):
    if not isinstance(value, int) or isinstance(value, bool):
        raise ConfigError("%s 必须是整数（实际 %r）" % (where, value))
    if value < lo or value > hi:
        raise ConfigError("%s=%r 越界（允许 %d..%d；fail-closed）"
                          % (where, value, lo, hi))
    return value


def _fixture_castle_name(name, where):
    if not isinstance(name, str) or not name.isdigit():
        raise ConfigError("%s 必须是十进制城目录名（如 \"0\"，实际 %r）" % (where, name))
    if str(int(name)) != name:
        raise ConfigError("%s=%r 不是规范十进制写法（前导零/负号都拒）" % (where, name))
    if int(name) > 99:
        raise ConfigError("%s=%r 越界（城目录名只允许 0..99）" % (where, name))
    return name


def _validate_fixture_files(fx, where):
    """文件夹具（配置覆盖 + 城堡前置）schema；返回归一化三件套。"""
    out = {"config_overrides": {}, "castle_list": None, "castle_files": {}, "files": {}}
    cfg = fx.get("config_overrides")
    if cfg is not None:
        if not isinstance(cfg, dict) or not cfg:
            raise ConfigError("%s.config_overrides 必须是非空对象（文件 → 键→值）" % where)
        for fname in sorted(cfg):
            w = "%s.config_overrides.%s" % (where, fname)
            if fname not in FIXTURE_CONFIG_FILES:
                raise ConfigError("%s: 未白名单的配置文件 %r（只认 %s）"
                                  % (w, fname, list(FIXTURE_CONFIG_FILES)))
            edits = cfg[fname]
            if not isinstance(edits, dict) or not edits:
                raise ConfigError("%s 必须是非空对象（键 → 目标值）" % w)
            norm = {}
            for key in sorted(edits):
                wk = "%s.%s" % (w, key)
                if key not in FIXTURE_CONFIG_KEYS:
                    raise ConfigError("%s: 未白名单的配置键 %r（白名单 %s）"
                                      % (wk, key, sorted(FIXTURE_CONFIG_KEYS)))
                lo, hi = FIXTURE_CONFIG_KEYS[key]
                norm[key] = _fixture_int(edits[key], wk, lo, hi)
            out["config_overrides"][fname] = norm
    cl = fx.get("castle_list")
    if cl is not None:
        if not isinstance(cl, list) or not cl:
            raise ConfigError("%s.castle_list 必须是非空数组" % where)
        if len(cl) > FIXTURE_CASTLE_MAX:
            raise ConfigError("%s.castle_list 最多 %d 座城（实际 %d）"
                              % (where, FIXTURE_CASTLE_MAX, len(cl)))
        names = [_fixture_castle_name(x, "%s.castle_list[%d]" % (where, i))
                 for i, x in enumerate(cl)]
        if len(set(names)) != len(names):
            raise ConfigError("%s.castle_list 有重复城：%s" % (where, names))
        out["castle_list"] = names
    cf = fx.get("castle_files")
    if cf is not None:
        if not isinstance(cf, dict) or not cf:
            raise ConfigError("%s.castle_files 必须是非空对象（城目录名 → 键值）" % where)
        for cname in sorted(cf):
            w = "%s.castle_files.%s" % (where, cname)
            _fixture_castle_name(cname, w)
            if out["castle_list"] is not None and cname not in out["castle_list"]:
                raise ConfigError("%s: 城 %r 不在同一 fixtures 的 castle_list %s 里"
                                  "（先列进 castle_list，否则目录可能不存在）"
                                  % (w, cname, out["castle_list"]))
            edits = cf[cname]
            if not isinstance(edits, dict) or not edits:
                raise ConfigError("%s 必须是非空对象（[Setup] 键 → 目标值）" % w)
            norm = {}
            for key in sorted(edits):
                wk = "%s.%s" % (w, key)
                if key not in FIXTURE_CASTLE_KEYS:
                    raise ConfigError("%s: 未白名单的 SabukW 键 %r（白名单 %s）"
                                      % (wk, key, sorted(FIXTURE_CASTLE_KEYS)))
                if FIXTURE_CASTLE_KEYS[key] == "int":
                    norm[key] = _fixture_int(edits[key], wk, 0, 0x7FFFFFFF)
                else:
                    val = edits[key]
                    if not isinstance(val, str):
                        raise ConfigError("%s 必须是字符串（实际 %r）" % (wk, val))
                    if "\r" in val or "\n" in val:
                        raise ConfigError("%s 不许含换行（会破坏 SabukW.txt）" % wk)
                    try:
                        raw = val.encode("gbk")
                    except UnicodeEncodeError as e:
                        raise ConfigError("%s=%r 不能编成 GBK：%s" % (wk, val, e))
                    if len(raw) > 32:
                        raise ConfigError("%s 的 GBK 字节数 %d > 32（超长会被 oracle 截断）"
                                          % (wk, len(raw)))
                    norm[key] = val
            out["castle_files"][cname] = norm
    fl = fx.get("files")
    if fl is not None:
        out["files"] = _validate_fixture_raw_files(fl, where)
    return out


def _validate_fixture_raw_files(fl, where):
    """`fixtures.files`：`{相对 Envir 的路径: 文本}` → `{rel: GBK 字节}`（卡 `openmir2-gm-script-fixture` A5）。

    约束（与审计 `9627d8cf` 一致）：
      * 相对路径，`/` 分隔，必须以 `Envir/` 开头；**禁绝对路径 / 盘符 / `..` 段**；
      * 文本必须能编成 **GBK**（线上脚本是 GBK 字节流），且非空；
      * 两侧声明同一份 ⇒ 由对称性检查兜底（`_fixture_symmetry` 已把 `files` 纳入比较）。
    """
    if not isinstance(fl, dict) or not fl:
        raise ConfigError("%s.files 必须是非空对象（相对 Envir 路径 → 文本）" % where)
    out = {}
    for rel in sorted(fl):
        w = "%s.files.%s" % (where, rel)
        if not isinstance(rel, str) or not rel.strip():
            raise ConfigError("%s 的路径必须是非空字符串" % w)
        norm = rel.replace("\\", "/").strip()
        if norm.startswith("/") or (len(norm) > 1 and norm[1] == ":"):
            raise ConfigError("%s: 不许绝对路径/盘符（只认相对 Envir 的路径）：%r" % (w, rel))
        parts = norm.split("/")
        if any(seg in ("", ".", "..") for seg in parts):
            raise ConfigError("%s: 路径段不许为空/`.`/`..`：%r" % (w, rel))
        if parts[0] != FIXTURE_FILES_ROOT:
            raise ConfigError("%s: 只许写 %s/ 下的文件（实际 %r）"
                              % (w, FIXTURE_FILES_ROOT, rel))
        text = fl[rel]
        if not isinstance(text, str) or not text:
            raise ConfigError("%s 的文本必须是非空字符串" % w)
        try:
            raw = text.encode("gbk")
        except UnicodeEncodeError as e:
            raise ConfigError("%s 的文本不能编成 GBK：%s" % (w, e))
        # **存 text 不存 bytes**（review C1/C2）：对称性与 summary 都会 json 序列化 decl。
        out["/".join(parts)] = text
    return out


def _validate_fixture_decl(decl, where):
    """统一 schema：`monsters` + 文件三件套 + `note`（至少给一个实体键）。"""
    if not isinstance(decl, dict) or not decl:
        raise ConfigError("%s 必须是非空对象（允许 keys=%s）" % (where, list(FIXTURE_KEYS)))
    unknown = sorted(set(decl) - set(FIXTURE_KEYS))
    if unknown:
        raise ConfigError("%s 有未知字段 %s（fail-closed；只允许 %s）"
                          % (where, unknown, list(FIXTURE_KEYS)))
    note = decl.get("note") or ""
    if not isinstance(note, str):
        raise ConfigError("%s.note 必须是字符串" % where)
    out = {"monsters": None, "config_overrides": {}, "castle_list": None,
           "castle_files": {}, "level": None, "inventory_replace": None,
           "inventory_drop": None, "note": note}
    if "monsters" in decl:
        out["monsters"] = _validate_monsters(decl["monsters"], where + ".monsters")
    out.update(_validate_fixture_files(decl, where))
    if "inventory_replace" in decl:
        out["inventory_replace"] = _validate_fixture_inv_replace(
            decl["inventory_replace"], where)
    if "inventory_drop" in decl:
        out["inventory_drop"] = _validate_fixture_inv_drop(decl["inventory_drop"], where)
    pos_r = {it["position"] for it in (out["inventory_replace"] or [])}
    pos_d = {it["position"] for it in (out["inventory_drop"] or [])}
    if pos_r & pos_d:
        raise ConfigError("%s: inventory_replace 与 inventory_drop 槽位重叠 %s"
                          % (where, sorted(pos_r & pos_d)))
    if "level" in decl:
        lv = decl["level"]
        if not isinstance(lv, int) or isinstance(lv, bool) or not (1 <= lv <= 255):
            raise ConfigError("%s.level 必须是 1..255 的整数（实际 %r；characters.Level 是 "
                              "int(11)，部署最大 255）" % (where, lv))
        out["level"] = lv
    if not (out["monsters"] or out["config_overrides"]
            or out["castle_list"] is not None or out["castle_files"] or out["files"]
            or out["level"] is not None or out["inventory_replace"]
            or out["inventory_drop"]):
        raise ConfigError("%s 只声明了 note：至少要给 monsters / config_overrides / "
                          "castle_list / castle_files / level 之一" % where)
    return out


def profile_fixtures(prof, side=None):
    """读窗口夹具声明：**顶层 `fixtures` 应用于两侧**；`sides.<side>.fixtures`
    **整体覆盖**本侧（不做深合并——免得两侧声明混在一起看不出谁生效）。

    形状非法 => ConfigError（`load_profile` 加载期就拒，与 `profile_magic` 同口径）。
    """
    if side is None:
        decl = prof.get("fixtures")
        where = "profile.fixtures"
    else:
        sc = (prof.get("sides") or {}).get(side) or {}
        if "fixtures" in sc:
            decl = sc["fixtures"]
            where = "profile.sides.%s.fixtures" % side
        else:
            decl = prof.get("fixtures")
            where = "profile.fixtures"
    if decl is None:
        return None
    return _validate_fixture_decl(decl, where)


def _fixture_symmetry_key(decl):
    """两侧必须一致的**六项**：castle 三件套（`castle_list` / `castle_files` /
    `config_overrides`）+ `level` + `inventory_replace` + `inventory_drop`。

    `monsters` **不**在这些里（怪物夹具允许按侧独立声明；其语义是「往本侧 MonGen 前插」，
    不改变两侧配置基线）。其余六项直接决定两侧运行时的城堡/配置/角色状态——不同就是对拍
    无效（W-TAX-2 尤其要求同值；inventory 写的是同一角色的背包槽）。
    """
    decl = decl or {}
    cl = decl.get("castle_list")
    return (tuple(cl) if cl is not None else None,
            json.dumps(decl.get("files") or {}, sort_keys=True),
            json.dumps(decl.get("castle_files") or {}, sort_keys=True,
                       ensure_ascii=False),
            json.dumps(decl.get("config_overrides") or {}, sort_keys=True,
                       ensure_ascii=False),
            decl.get("level"),
            json.dumps(decl.get("inventory_replace") or [], sort_keys=True,
                       ensure_ascii=False),
            json.dumps(decl.get("inventory_drop") or [], sort_keys=True,
                       ensure_ascii=False))


def _check_fixture_symmetry(fixtures_by_side):
    """两侧**夹具声明**（castle 三件套 / `level` / `inventory_replace` / `inventory_drop`）
    必须一致。

    为什么必须在这里拦：`castle_list_symmetry` 门（preflight）跑在**起栈前**，看的是夹具
    写盘**之前**的 `Castle/List.txt`；而夹具在 `run_side` 里 restore 之后、起栈之前才按侧写，
    且 `sides.<side>.fixtures` **整体覆盖**顶层 ⇒ 两侧各写不同内容时，门永远看不到。
    ⇒ 门守「文件现状」，本函数守「声明未来」，两者缺一不可。两侧值不同 ⇒ 对拍无效（假绿），
    必须 fail-closed（review F-1；reviewer-20 的 supersede 建议把六项一起比）。
    """
    keys = {}
    for side in sorted(fixtures_by_side):
        keys[side] = _fixture_symmetry_key(fixtures_by_side[side])
    if len(set(keys.values())) > 1:
        shown = []
        for side in sorted(keys):
            decl = fixtures_by_side[side] or {}
            shown.append("%s{castle_list=%r, castle_files=%r, config_overrides=%r, "
                         "level=%r, inventory_replace=%r, inventory_drop=%r}"
                         % (side, decl.get("castle_list"),
                            decl.get("castle_files") or {},
                            decl.get("config_overrides") or {},
                            decl.get("level"),
                            decl.get("inventory_replace"),
                            decl.get("inventory_drop")))
        raise ConfigError(
            "两侧夹具不一致（castle 三件套 / level / inventory）（%s）：`castle_list` 会让 "
            "`castle_list_symmetry` 门失效（门在写盘**之前**检查，夹具在写**之后**生效）；"
            "`castle_files`/`config_overrides`/`level`/`inventory_*` 不同则两侧运行时状态"
            "不同、对拍无效 ⇒ 把这些项只写在**顶层 fixtures**（两侧同值）；若某侧必须用 "
            "`sides.<side>.fixtures` 覆盖，也要把同一份声明一起带上"
            % ", ".join(shown))


def _guard_prod_target(path):
    """**逐目标**生产根守卫（卡 `openmir2-gm-script-fixture` F1）：files 与 monsters 复用。"""
    real_path = os.path.realpath(path)
    real_prod = os.path.realpath(FIXTURE_PROD_ROOT)
    if real_path == real_prod or real_path.startswith(real_prod + os.sep):
        raise ConfigError("夹具目标解析到生产内容根（%s ⇒ %s）⇒ 拒绝（内容冻结）"
                          % (path, real_path))


def _fixture_envir_root(sc):
    root = sc.get("envir_root")
    if not root:
        raise ConfigError("fixtures 需要 profile.sides.*.envir_root（当前未声明）")
    root = str(root)
    real = os.path.realpath(root)
    prod = os.path.realpath(FIXTURE_PROD_ROOT)
    if real == prod or real.startswith(prod + os.sep):
        raise ConfigError(
            "envir_root 指向生产内容根（%s）：窗口夹具只许写私有根（内容冻结）" % root)
    return root


def _fixture_envir_roots(sc):
    """夹具要写的 envir 根列表：基线 + **hook 影子**（存在时）。

    G-1（review）：`--rng-log` 时 C# GameSvr 从 `M2GameSvr-hook/`（影子）跑，而 profile 的
    `envir_root` 指基线 `M2GameSvr/`。只写基线会让①夹具对 oracle 不生效、②影子失配 ⇒
    下一轮 `envir_shadow_symmetry` 必红。故影子存在时**两侧写同一份内容**，保持 sha 一致。
    """
    base = _fixture_envir_root(sc)
    roots = [base]
    parent = os.path.dirname(os.path.abspath(base))
    shadow = os.path.join(parent, "M2GameSvr-hook")
    if os.path.isdir(shadow) and os.path.isfile(os.path.join(shadow, "Envir", "MonGen.txt")):
        _fixture_envir_root({"envir_root": shadow})  # 影子同样过生产根守卫
        roots.append(shadow)
    return roots


def _fixture_map_names(root):
    path = os.path.join(root, "Envir", "MapInfo.txt")
    try:
        with open(path, "rb") as f:
            raw = f.read()
    except OSError as e:
        raise ConfigError("读 MapInfo.txt 失败：%s（%s）" % (path, e))
    names = set()
    for line in raw.decode("gbk", "replace").splitlines():
        line = line.strip()
        if not line.startswith("["):
            continue
        body = line[1:].split("]", 1)[0]
        tok = body.split()[0] if body.split() else ""
        if tok:
            names.add(tok)
    if not names:
        raise ConfigError("MapInfo.txt 解析不到任何地图名：%s" % path)
    return names


def _fixture_targets(decl):
    """文件夹具 → [(rel, kind, payload)]；rel 一律 `/` 分隔（备份文件名可移植）。"""
    out = []
    for fname in sorted(decl["config_overrides"]):
        out.append((fname, "ini", sorted(decl["config_overrides"][fname].items())))
    if decl["castle_list"] is not None:
        out.append(("Castle/List.txt", "castle_list", list(decl["castle_list"])))
    for cname in sorted(decl["castle_files"]):
        out.append(("Castle/%s/SabukW.txt" % cname, "ini",
                    sorted(decl["castle_files"][cname].items())))
    for rel in sorted(decl.get("files") or {}):
        out.append((rel, "raw", decl["files"][rel]))
    return out


def _fixture_ini_replace(data, edits, where):
    """只替换 `Key=Value` 行的值（保留 key 原文/缩进/行尾）。

    要求该键在**整个文件里恰好出现 1 次**，且值后没有空白/注释——否则 fail-closed
    （不替操作者猜要保留什么）。
    """
    lines = data.splitlines(keepends=True)
    out = list(lines)
    diffs = []
    for key, value in edits:
        pat = key.encode("ascii") + b"="
        hits = [i for i, ln in enumerate(out) if ln.lstrip(b" \t").startswith(pat)]
        if len(hits) != 1:
            raise ConfigError("%s: 键 %s 在文件里出现 %d 次（要求恰好 1 次；不猜）"
                              % (where, key, len(hits)))
        i = hits[0]
        ln = out[i]
        stripped = ln.lstrip(b" \t")
        lead = ln[:len(ln) - len(stripped)]
        body = stripped[len(pat):]
        m = re.search(br"(\r\n|\n|\r)?$", body)
        ending = m.group(1) or b""
        raw_val = body[:len(body) - len(ending)]
        if raw_val != raw_val.strip() or b";" in raw_val or b"#" in raw_val:
            raise ConfigError("%s: 键 %s 的值后带空白/注释（%r）——不替操作者猜，"
                              "先把它清成裸 `Key=Value` 再声明覆盖"
                              % (where, key, raw_val))
        raw_new = (value.encode("gbk") if isinstance(value, str)
                   else str(value).encode("ascii"))
        out[i] = lead + pat + raw_new + ending
        diffs.append((ln.decode("gbk", "replace").rstrip("\r\n"),
                      out[i].decode("gbk", "replace").rstrip("\r\n")))
    return b"".join(out), diffs


def _fixture_new_bytes(kind, payload, data, where):
    if kind == "castle_list":
        nl = b"\r\n" if b"\r\n" in data else b"\n"
        body = nl.join(n.encode("ascii") for n in payload)
        if data.endswith((b"\n", b"\r")):
            body += nl
        old = data.decode("ascii", "replace").split()
        return body, [("castle_list=%r" % old, "castle_list=%r" % list(payload))]
    if kind == "raw":
        raw = payload.encode("gbk") if isinstance(payload, str) else payload
        return raw, [("raw(%dB)" % len(data), "raw(%dB)" % len(raw))]
    return _fixture_ini_replace(data, payload, where)


def _fixture_atomic_write(path, data):
    tmp = path + ".fixture-tmp"
    try:
        with open(tmp, "wb") as f:
            f.write(data)
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            try:
                os.remove(tmp)
            except OSError:
                pass


def _rollback_file_recs(recs):
    """把已写文件按备份回滚并核对 sha256（就地回滚用）。返回错误列表。"""
    errs = []
    for rec in recs:
        if not rec.get("backup"):
            continue
        if rec.get("created"):
            # 新建型：回滚 = 删除（写前状态是「不存在」）。
            try:
                if os.path.isfile(rec["path"]):
                    os.remove(rec["path"])
                if rec.get("backup") and os.path.isfile(rec["backup"]):
                    os.remove(rec["backup"])
                rec["backup"] = None
                rec["restore_sha256"] = "absent"
                rec["action"] = "rolled-back-created-removed"
            except OSError as e:
                errs.append("%s 回滚失败（删除新建文件）：%s" % (rec["rel"], e))
            continue
        try:
            with open(rec["backup"], "rb") as f:
                data = f.read()
            if hashlib.sha256(data).hexdigest() != rec["sha256_before"]:
                errs.append("%s 的备份 sha256 不符" % rec["rel"])
                continue
            _fixture_atomic_write(rec["path"], data)
            with open(rec["path"], "rb") as f:
                got = hashlib.sha256(f.read()).hexdigest()
            rec["restore_sha256"] = got
            if got != rec["sha256_before"]:
                errs.append("%s 回滚后 sha256=%s != 写前 %s"
                            % (rec["rel"], got, rec["sha256_before"]))
                continue
            os.remove(rec["backup"])
            rec["backup"] = None
            rec["action"] = "rolled-back"
        except OSError as e:
            errs.append("%s 回滚失败：%s" % (rec["rel"], e))
    return errs


def _apply_files(runner, side, root, decl, side_dir, tag, sink_targets=None,
                 sink_files=None):
    """在**一个根**上写配置覆盖/城堡前置（备份 + 原子写 + 读回）；失败就地回滚后抛。

    返回 (targets, backup_dir)：targets 每项带 `target`（baseline/shadow）、`rel`、
    before/after sha256、`changed`、`diff`，与怪物夹具的 targets 同形状（fixtures.json 合一）。
    """
    planned = []
    for rel, kind, payload in _fixture_targets(decl):
        path = os.path.join(root, *rel.split("/"))
        _guard_prod_target(path)
        created = False
        if not os.path.isfile(path):
            # 卡 `openmir2-gm-script-fixture` A5：`files`（raw）允许**新建**脚本文件；
            # 其余类仍要求目标已存在（不创建/不猜）。
            if kind != "raw":
                raise ConfigError("夹具目标文件不存在：%s（不创建/不猜；先建私有根夹具）"
                                  % path)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            data = b""
            created = True
        else:
            with open(path, "rb") as f:
                data = f.read()
        new, diffs = _fixture_new_bytes(kind, payload, data, "%s:%s" % (side, rel))
        planned.append({"rel": rel, "path": path, "old": data, "new": new,
                        "diff": diffs, "created": created})
    if getattr(runner, "dry_run", False):
        for p in planned:
            for old, new in p["diff"]:
                runner.log("[%s] 夹具 %s/%s: - %s" % (side, tag, p["rel"], old))
                runner.log("[%s] 夹具 %s/%s: + %s" % (side, tag, p["rel"], new))
        return ([{"target": tag, "rel": p["rel"], "path": p["path"],
                  "backup": None,
                  "before_sha256": hashlib.sha256(p["old"]).hexdigest(),
                  "after_sha256": hashlib.sha256(p["new"]).hexdigest(),
                  "changed": p["old"] != p["new"], "diff": p["diff"],
                  "action": "dry-run", "restore_sha256": None}
                 for p in planned], None)
    backup_dir = os.path.join(side_dir, "fixtures", "backup")
    os.makedirs(backup_dir, exist_ok=True)
    recs = []
    try:
        for p in planned:
            bpath = os.path.join(backup_dir, "%s__%s" % (tag,
                                                         p["rel"].replace("/", "__")))
            with open(bpath, "wb") as f:
                f.write(p["old"])
            before_sha = hashlib.sha256(p["old"]).hexdigest()
            rec = {"target": tag, "rel": p["rel"], "path": p["path"],
                   "created": p.get("created", False),
                   "backup": bpath,
                   "before_sha256": before_sha, "sha256_before": before_sha,
                   "after_sha256": None, "changed": p["old"] != p["new"],
                   "diff": p["diff"], "action": "applied", "restore_sha256": None}
            recs.append(rec)
            # 卡 `openmir2-gm-script-fixture` A4：**先登记后写**——即使第 N 个写失败、
            # 就地回滚，证据里仍有逐文件记录（before/after/restore sha 可对账）。
            if sink_targets is not None:
                sink_targets.append(rec)
            if sink_files is not None:
                sink_files.append(rec)
            if rec["changed"]:
                _fixture_atomic_write(p["path"], p["new"])
            with open(p["path"], "rb") as f:
                got = f.read()
            rec["after_sha256"] = hashlib.sha256(got).hexdigest()
            rec["sha256_before"] = rec["before_sha256"]
            rec["sha256_after"] = rec["after_sha256"]
            if got != p["new"]:
                raise ConfigError("%s 写后读回与目标不符（sha %s != %s）"
                                  % (p["path"], rec["after_sha256"],
                                     hashlib.sha256(p["new"]).hexdigest()))
    except (ConfigError, OSError) as e:
        # review F1：逐文件写时第 N 个失败，前 N-1 个已经落盘；外层只能看到函数返回后的
        # 记录 ⇒ 必须在这里就地回滚（`restore-baseline*.sh` 只管 DB，不覆盖配置文件）。
        rollback_errs = _rollback_file_recs(recs)
        if rollback_errs:
            raise ConfigError("%s；且就地回滚不完整：%s" % (e, "；".join(rollback_errs)))
        raise
    runner.log("[%s] 文件夹具（起栈前写 %s）：%d 个文件；备份目录=%s"
               % (side, tag, len(recs), backup_dir))
    for rec in recs:
        runner.log("[%s]   %s/%s sha256 %s -> %s%s"
                   % (side, tag, rec["rel"], rec["before_sha256"][:12],
                      rec["after_sha256"][:12],
                      "" if rec["changed"] else "（noop）"))
    return recs, backup_dir


def run_level_fixture(runner, side, prof, sc, level, env, side_dir, timeout=120):
    """起栈前把测试角色 `characters.Level` 设成声明值（写前快照 → 写 → 读回校验）。

    与 assets 通道同族（定位 ChrName+LoginID、其余列 MD5 指纹、写后读回断言）；
    停栈后的还原由 `restore_level_fixture` 负责。失败一律 rc=2，绝不带着错位继续起栈。
    """
    res = {"side": side, "declared": level, "action": None, "db": None,
           "player_id": None, "before_level": None, "after_level": None,
           "sql": {}, "rc": None, "error": None}
    try:
        db = placement_characters_db(side, sc)
        res["db"] = db
        res["db_pairing"] = _check_db_pairing(side, placement_envir_root(side, sc), db)
        client = placement_db_client(prof, side)
    except ConfigError as e:
        res["error"] = "等级夹具前置校验失败：%s" % e
        res["rc"] = 2
        return res
    argv = _mysql_argv(client)
    # review：凭据（client.password / --password=… 的 argv）**不得进证据 dict**；
    # 只放在内存 `_session`，写盘一律经 `level_evidence()` 过滤。
    res["_session"] = {"client": client, "argv": argv}
    chr_name = str(prof.get("char_name") or "")
    login_id = str(prof.get("account") or "")
    if not chr_name or not login_id:
        res["error"] = "等级夹具需要 profile 的 char_name/account"
        res["rc"] = 2
        return res
    if getattr(runner, "dry_run", False):
        res["action"] = "dry-run"
        res["sql"] = {"update": "UPDATE `%s`.`characters` SET `Level`=%d WHERE Id=<row>"
                               % (db, level)}
        runner.log("[%s] 等级夹具（dry-run 不连库）：db=%s UPDATE characters SET Level=%d "
                   "WHERE Id=<row> 角色=%s" % (side, db, level, chr_name))
        res["rc"] = 0
        return res

    def sql(tag, text):
        rc, out, err = runner.run(side, tag, argv, env=env, timeout=timeout,
                                  stdin_text=text, capture=True)
        if rc != 0:
            raise ConfigError("%s 执行失败（mysql rc=%s）：%s"
                              % (tag, rc, (err or out or "").strip()[:400]))
        return out

    try:
        cols_out = sql("level.columns", "SHOW COLUMNS FROM `%s`.`characters`;\n" % db)
        columns = [ln.split("\t")[0] for ln in cols_out.splitlines() if ln.strip()]
        if "Level" not in columns:
            raise ConfigError("%s.characters 缺 Level 列（表结构与预期不符，拒绝）" % db)
        read_sql = _assets_row_sql(db, chr_name, login_id, columns, ["Level"])
        res["sql"]["read"] = read_sql.strip()
        before = _parse_assets_row(sql("level.read", read_sql), 1)
        res["player_id"] = before["id"]
        res["before_level"] = before["values"][0]
        if before["count"] != 1 or before["deleted"] == 1:
            raise ConfigError("等级夹具定位失败：命中 %d 行 / Deleted=%d（ChrName=%r）"
                              % (before["count"], before["deleted"], chr_name))
        if before["values"] == [level]:
            res["action"] = "noop"
            res["after_level"] = level
            res["rc"] = 0
            return res
        upd = ("UPDATE `%s`.`characters` SET `Level` = %d WHERE `Id` = %d;\n"
               % (db, level, before["id"]))
        res["sql"]["update"] = upd.strip()
        out = sql("level.update", upd)
        if str(out).strip() not in ("1", ""):
            raise ConfigError("等级夹具 UPDATE 影响了 %r 行（期望 1）" % str(out).strip())
        after = _parse_assets_row(sql("level.verify", read_sql), 1)
        res["after_level"] = after["values"][0]
        if after["values"] != [level]:
            raise ConfigError("等级夹具读回与目标不符：Level=%r != %r"
                              % (after["values"][0], level))
        if before["fp"] != after["fp"]:
            raise ConfigError("等级夹具写后其它列指纹变化（连带写坏）")
        res["action"] = "updated"
        res["rc"] = 0
        return res
    except (ConfigError, OSError) as e:
        res["error"] = "等级夹具失败：%s" % e
        res["rc"] = 2
        return res


def level_evidence(res):
    """可落盘的等级夹具证据：剔除所有 `_` 前缀键（`_session` 含口令/argv），并二次脱敏。"""
    if not res:
        return res
    out = {k: v for k, v in res.items() if not str(k).startswith("_")}
    blob = json.dumps(out, ensure_ascii=False)
    if "--password" in blob or '"password"' in blob:
        # 理论上不会发生；防御性兜底，绝不把口令写进证据。
        out["redacted"] = True
        for k in list(out):
            if isinstance(out[k], str) and "--password" in out[k]:
                out[k] = _redact_argv([out[k]])[0]
    return out


def restore_level_fixture(runner, res, env, timeout=120):
    """停栈后把 `Level` 还原到写前值并**重读**校验；失败返回错误串（调用方升级 rc=2）。"""
    if not res or res.get("action") not in ("updated",):
        return None
    try:
        sess = res.get("_session") or {}
        client = sess.get("client")
        db = res.get("db")
        argv = sess.get("argv")
        if not client or not db or not argv:
            return "等级夹具还原缺少连接信息"
        upd = ("UPDATE `%s`.`characters` SET `Level` = %d WHERE `Id` = %d;\n"
               % (db, int(res["before_level"]), int(res["player_id"])))
        rc, out, err = runner.run(res["side"], "level.restore", argv, env=env,
                                  timeout=timeout, stdin_text=upd, capture=True)
        if rc != 0:
            return "等级夹具还原 UPDATE 失败（mysql rc=%s）：%s" % (
                rc, (err or out or "").strip()[:300])
        rc, out, err = runner.run(res["side"], "level.restore_verify", argv, env=env,
                                  timeout=timeout, stdin_text=res["sql"]["read"], capture=True)
        if rc != 0:
            return "等级夹具还原后读回失败（mysql rc=%s）" % rc
        after = _parse_assets_row(out, 1)
        if after["values"] != [int(res["before_level"])]:
            return "等级夹具还原后 Level=%r != 写前 %r" % (
                after["values"][0], res["before_level"])
        return None
    except (ConfigError, OSError, ValueError, KeyError) as e:
        return "等级夹具还原失败：%s" % e


def run_fixtures(runner, side, prof, sc, decl, env, side_dir, timeout=120):
    """起栈前应用**全部**窗口夹具（怪物 + 文件），对**每个根**（基线 + hook 影子）各写一份。

    返回证据 dict：`targets[]`（怪物/文件统一形状：target/path/backup/before/after）、
    `monsters`/`files` 便捷视图、`action`（prepended / applied / prepended+applied / dry-run）。
    怪物行**前插**到 `MonGen.txt` 头部（`FIXTURE_MONGEN_LAYOUT`）：追加到单表 3081 行末尾
    扫一轮 ≈ 616s，窗口内轮不到（刷怪线程每 200ms 只前进一条）。
    失败 fail-closed（rc=2 + 就地回滚已写项）；停栈后的最终还原由 `restore_fixtures` 负责。
    """
    res = {"side": side, "source": "profile", "declared": decl, "envir_root": None,
           "roots": [], "backup_dir": None, "path": None, "backup": None,
           "before_sha256": None, "after_sha256": None, "prepend": [],
           "files": [], "targets": [], "action": None, "rc": None, "error": None}
    dry = getattr(runner, "dry_run", False)
    try:
        roots = _fixture_envir_roots(sc)
        res["envir_root"] = roots[0]
        res["roots"] = list(roots)
        did_m = did_f = False
        monster_lines = []
        if decl.get("monsters"):
            base = roots[0]
            maps = _fixture_map_names(base)
            for spec in decl["monsters"]:
                if spec["map"] not in maps:
                    raise ConfigError("fixtures 指定的地图 %r 不在本侧 MapInfo.txt"
                                      "（fail-closed）" % spec["map"])
                monster_lines.append("\t".join([
                    spec["map"], str(spec["x"]), str(spec["y"]), spec["name"], "0",
                    str(spec["count"]), str(FIXTURE_MONGEN_ZEN_TIME)]))
            res["prepend"] = monster_lines
            if dry:
                for root in roots:
                    for ln in monster_lines:
                        runner.log("[%s] 怪物夹具（dry-run 不写 Envir，布局=%s）："
                                   "%s/Envir/MonGen.txt ⇤ %s"
                                   % (side, FIXTURE_MONGEN_LAYOUT, root, ln))
            else:
                backup_dir = os.path.join(side_dir, "fixtures")
                os.makedirs(backup_dir, exist_ok=True)
                for i, root in enumerate(roots):
                    path = os.path.join(root, "Envir", "MonGen.txt")
                    if not os.path.exists(path):
                        raise ConfigError("MonGen.txt 不存在：%s" % path)
                    _guard_prod_target(path)
                    with open(path, "rb") as f:
                        raw = f.read()
                    before = hashlib.sha256(raw).hexdigest()
                    nl = b"\r\n" if b"\r\n" in raw else b"\n"
                    body = raw if (not raw or raw.endswith(b"\n")) else raw + nl
                    block = b"".join(ln.encode("gbk") + nl for ln in monster_lines)
                    tag = "shadow" if i else "baseline"
                    backup = os.path.join(backup_dir, "MonGen.txt.%s.bak" % tag)
                    with open(backup, "wb") as f:
                        f.write(raw)
                    tgt = {"target": tag, "path": path, "backup": backup,
                           "layout": FIXTURE_MONGEN_LAYOUT,
                           "before_sha256": before, "after_sha256": None}
                    # review F1：**先登记后写**——写失败/机判失败时外层 except 的
                    # restore_fixtures 才能按这份备份回滚（否则文件静默残留新内容）。
                    res["targets"].append(tgt)
                    # prepend（关键）：夹具行在 index 0，首个刷怪闸门就取到。
                    _mongen_write_layout(path, body, block)
                    with open(path, "rb") as f:
                        after = f.read()
                    tgt["after_sha256"] = hashlib.sha256(after).hexdigest()
                    ok, why = _mongen_fixture_layout(raw, after, monster_lines)
                    if not ok:
                        raise ConfigError("MonGen.txt 夹具布局机判失败（%s）：%s"
                                          "（已按备份回滚）" % (tag, why))
                    runner.log("[%s] 怪物夹具（起栈前**前插** %s/Envir/MonGen.txt）："
                               "%d 行位于文件头；sha256 %s -> %s；备份=%s"
                               % (side, tag, len(monster_lines), before[:12],
                                  tgt["after_sha256"][:12], backup))
                    for ln in monster_lines:
                        runner.log("[%s]   + %s" % (side, ln))
                first = res["targets"][0]
                res.update({"path": first["path"], "backup": first["backup"],
                            "before_sha256": first["before_sha256"],
                            "after_sha256": first["after_sha256"]})
            did_m = True
        if (decl.get("config_overrides") or decl.get("castle_list") is not None
                or decl.get("castle_files") or decl.get("files")):
            for i, root in enumerate(roots):
                tag = "shadow" if i else "baseline"
                _recs, bdir = _apply_files(runner, side, root, decl, side_dir, tag,
                                           sink_targets=res["targets"],
                                           sink_files=res["files"])
                if bdir:
                    res["backup_dir"] = bdir
            did_f = True
        if dry:
            res["action"] = "dry-run"
        else:
            res["action"] = ("prepended+applied" if (did_m and did_f)
                             else "prepended" if did_m else "applied")
        res["rc"] = 0
        return res
    except (ConfigError, OSError) as e:
        res["error"] = "窗口夹具失败：%s" % e
        res["rc"] = 2
        err2 = restore_fixtures(res)
        res["rollback_error"] = err2
        if err2:
            res["error"] += "；回滚也不完整：%s" % err2
        return res


def preview_fixtures(runner, side, prof, sc, decl, ctx):
    """`--dry-run`：只打印 planned diff（`run_fixtures` 的 dry-run 分支不写文件）。"""
    res = run_fixtures(runner, side, prof, sc, decl, os.environ.copy(), None)
    return res, res.get("rc") != 0


def restore_fixtures(res):
    """按 `targets[]` 逐个还原（怪物 + 文件；基线 + 影子），重读磁盘核对 sha256。

    返回 None（成功）或错误字符串。**rc 由调用方决定**（`run_side` 的 finally 把
    还原失败升级为 rc=2）。
    """
    if not res:
        return None
    targets = res.get("targets") or []
    if not targets and res.get("backup"):
        targets = [{"target": "baseline", "path": res.get("path"),
                    "backup": res.get("backup"),
                    "before_sha256": res.get("before_sha256")}]
    errs = []
    for tgt in targets:
        if not tgt.get("backup"):
            # dry-run / 已回滚 / noop 记录
            continue
        name = tgt.get("rel") or "MonGen.txt"
        label = "%s/%s" % (tgt.get("target") or "baseline", name)
        if tgt.get("created"):
            # 卡 `openmir2-gm-script-fixture` A4：夹具新建的文件，还原 = **删除**并断言不存在
            # （写前状态就是「不存在」⇒ before_sha 不存在，restore 后必须仍不存在）。
            try:
                if os.path.isfile(tgt["path"]):
                    os.remove(tgt["path"])
                if os.path.exists(tgt["path"]):
                    errs.append("%s 还原失败：删除后仍存在" % label)
                else:
                    if tgt.get("backup") and os.path.isfile(tgt["backup"]):
                        os.remove(tgt["backup"])
                    tgt["backup"] = None
                    tgt["restore_sha256"] = "absent"
                    tgt["action"] = "restored-created-removed"
            except OSError as e:
                errs.append("%s 还原失败（删除新建文件）：%s" % (label, e))
            continue
        try:
            with open(tgt["backup"], "rb") as f:
                data = f.read()
            sha = hashlib.sha256(data).hexdigest()
            if sha != tgt["before_sha256"]:
                errs.append("%s 的备份 sha256=%s 与写前 %s 不符（备份被改过）"
                            % (label, sha, tgt["before_sha256"]))
                continue
            _fixture_atomic_write(tgt["path"], data)
            with open(tgt["path"], "rb") as f:
                got = hashlib.sha256(f.read()).hexdigest()
            tgt["restore_sha256"] = got
            if got != tgt["before_sha256"]:
                errs.append("%s 还原后 sha256=%s != 写前 %s"
                            % (label, got, tgt["before_sha256"]))
                continue
            os.remove(tgt["backup"])
            tgt["backup"] = None
            tgt["action"] = "restored"
        except OSError as e:
            errs.append("%s 还原失败：%s" % (label, e))
    if not errs:
        if res.get("targets"):
            first = next((t for t in res["targets"] if t.get("restore_sha256")),
                         None)
            if first:
                res["restore_sha256"] = first["restore_sha256"]
        res["backup"] = None
    return ("；".join(errs) if errs else None)


def run_side(runner, side, prof, ctx, password, placement=None, inventory=None,
             assets=None, user_vars=None):
    sc = prof["sides"][side]
    side_dir = ctx["side_dir"]
    db_dir = ctx["db_dir"]
    res = {
        "side": side,
        "label": sc.get("label") or side,
        "restore_rc": None,
        "placement": None,
        "magic": None,
        "inventory": None,
        "assets": None,
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
    if ctx.get("rng_log"):
        # 卡 openmir2-window-kit-p0-unblock：每侧各自一本账（Rust `-Seed 42` 已在起栈脚本里；
        # C# 侧需把 rng-hook 部署进私栈并用同一变量名，见 docs/window-kit-p0-unblock-20261025.md）。
        # review P-1：必须同时给 `MIR2_RNG_SITE=1`，否则 C# 钩子写 4 列（无 site）⇒ 校验必红。
        env.update(rng_env(side_dir))
        res["rng_log"] = env["MIR2_RNG_LOG"]

    os.makedirs(side_dir, exist_ok=True)
    os.makedirs(db_dir, exist_ok=True)

    stack_attempted = False
    level_state = None  # 等级夹具的内存态（含凭据）；落盘只用 level_evidence()
    inventory_fixture_state = None  # 背包替换/丢弃夹具内存态（含凭据）
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

        # 1b) 背包夹具（卡 openmir2-window-inventory-fixture）：同样在 restore 之后、
        # 起栈之前；失败即 rc=2 点名，绝不带着错位继续 probe。
        if inventory:
            ires = run_inventory(runner, side, prof, sc, inventory, env)
            try:
                write_json_file(os.path.join(side_dir, "inventory.json"), ires)
            except OSError as e:
                ires["error"] = ((ires.get("error") or "") + "；写 inventory.json 失败：%s"
                                 % e).strip("；")
                ires["rc"] = 2
            res["inventory"] = ires
            if ires.get("rc") != 0:
                res["error"] = ires.get("error") or "背包夹具失败"
                return res

        # 1b) 资产声明通道（卡 openmir2-window-gold-declaration-channel）：同样在
        #     restore 之后、起栈之前；失败即 rc=2 点名，绝不带着错位继续 probe。
        if assets:
            ares = run_assets(runner, side, prof, sc, assets, env)
            try:
                write_json_file(os.path.join(side_dir, "assets.json"), ares)
            except OSError as e:
                ares["error"] = ((ares.get("error") or "") + "；写 assets.json 失败：%s"
                                 % e).strip("；")
                ares["rc"] = 2
            res["assets"] = ares
            if ares.get("rc") != 0:
                res["error"] = ares.get("error") or "资产声明通道失败"
                return res

        # 1c) 魔法夹具（profile 声明了才跑）：restore（与预置位）之后、起栈之前。
        try:
            magic_decl = profile_magic(prof, side)
        except ConfigError as e:
            res["error"] = "魔法夹具声明非法：%s" % e
            return res
        if magic_decl:
            mres = run_magic(runner, side, prof, sc, magic_decl, env)
            try:
                write_json_file(os.path.join(side_dir, "magic.json"), mres)
            except OSError as e:
                mres["error"] = ((mres.get("error") or "") + "；写 magic.json 失败：%s"
                                 % e).strip("；")
                mres["rc"] = 2
            res["magic"] = mres
            if mres.get("rc") != 0:
                res["error"] = mres.get("error") or "魔法夹具失败"
                return res

        # 1d) 窗口夹具（`profile.fixtures`，可被 `sides.<side>.fixtures` 整体覆盖）：
        # 怪物（MonGen.txt）+ 配置覆盖 + 城堡前置；**每个根**（基线 + hook 影子）各写一份，
        # 停栈后由 finally 的 restore_fixtures 按 sha256 还原；失败 fail-closed。
        try:
            fixtures_decl = profile_fixtures(prof, side)
        except ConfigError as e:
            res["error"] = "窗口夹具声明非法：%s" % e
            return res
        if fixtures_decl:
            fres = run_fixtures(runner, side, prof, sc, fixtures_decl, env, side_dir)
            try:
                write_json_file(os.path.join(side_dir, "fixtures.json"), fres)
            except OSError as e:
                fres["error"] = ((fres.get("error") or "") + "；写 fixtures.json 失败：%s"
                                 % e).strip("；")
                fres["rc"] = 2
            res["fixtures"] = fres
            if fres.get("rc") != 0:
                res["error"] = fres.get("error") or "窗口夹具失败"
                return res
            # 等级夹具（DB 侧）：restore 之后、起栈之前；写前快照 + 读回 + 停栈后还原。
            if fixtures_decl.get("level") is not None:
                lres = run_level_fixture(runner, side, prof, sc, fixtures_decl["level"],
                                         env, side_dir)
                level_state = lres
                try:
                    write_json_file(os.path.join(side_dir, "level.json"),
                                    level_evidence(lres))
                except OSError as e:
                    lres["error"] = ((lres.get("error") or "") + "；写 level.json 失败：%s"
                                     % e).strip("；")
                    lres["rc"] = 2
                res["level"] = level_evidence(lres)
                if lres.get("rc") != 0:
                    res["error"] = lres.get("error") or "等级夹具失败"
                    return res
            # 背包替换/丢弃夹具（DB 侧，卡 openmir2-window-fixture-inventory）：同款
            # 「起栈前写 + 停栈后整行还原」；失败 fail-closed。
            if (fixtures_decl.get("inventory_replace")
                    or fixtures_decl.get("inventory_drop")):
                inventory_fixture_state = run_inventory_fixture(
                    runner, side, prof, sc, fixtures_decl, env, side_dir)
                try:
                    write_json_file(os.path.join(side_dir, "inventory_fixture.json"),
                                    level_evidence(inventory_fixture_state))
                except OSError as e:
                    inventory_fixture_state["error"] = (
                        (inventory_fixture_state.get("error") or "")
                        + "；写 inventory_fixture.json 失败：%s" % e).strip("；")
                    inventory_fixture_state["rc"] = 2
                res["inventory_fixture"] = level_evidence(inventory_fixture_state)
                if inventory_fixture_state.get("rc") != 0:
                    res["error"] = (inventory_fixture_state.get("error")
                                    or "背包替换/丢弃夹具失败")
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

        # 6) probe（带上本侧 discovery 推导出的 --var + 操作者透传的 --var）
        probe_argv = build_probe_argv(
            sc, ctx, prof, password,
            vars_argv=list(vars_argv) + list(user_vars or []))
        res["probe_rc"] = runner.run(side, "probe", probe_argv, env=env,
                                     timeout=probe_timeout)
        # 6b) 存档落定等待（卡 `openmir2-harness-save-time-symmetry`）：probe 关连接后，
        #     Rust 在 gate 事件里同步存档、C# 走异步 FrontEngine→DBSrv；拍库前必须两侧都
        #     到「可观测静默」，否则会把「一侧已落库、另一侧仍是基线」误读成行为差异。
        #     窗口内不落定 ⇒ fail-closed（rc=2 save_unsettled），**不拍库**。
        try:
            settle_cfg = profile_save_settle(prof, side)
        except ConfigError as e:
            res["error"] = "存档落定等待配置非法：%s" % e
            return res
        sres = wait_save_settled(runner, side, prof, sc, settle_cfg, env)
        try:
            write_json_file(os.path.join(side_dir, "save_settle.json"), sres)
        except OSError as e:
            sres["error"] = ((sres.get("error") or "") + "；写 save_settle.json 失败：%s"
                             % e).strip("；")
            sres["rc"] = 2
        res["save_settle"] = sres
        if sres.get("rc") != 0:
            res["error"] = sres.get("error") or "存档落定等待失败"
            return res

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
        if ctx.get("rng_log"):
            ok, detail = check_rng_ledger(res["rng_log"])
            res["rng_log_ok"] = ok
            res["rng_log_detail"] = detail
            if not ok:
                res["error"] = "RNG 账本不可用：%s" % detail
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
        # 8b) 还原窗口夹具（怪物 + 文件；**停栈之后**——服务端运行时会改写
        # `Castle/<n>/SabukW.txt`，栈没停就还原会被覆盖）。还原失败 => 该侧 rc=2
        # （配置文件留在被改状态会污染下一轮）。
        if res.get("fixtures"):
            ferr = restore_fixtures(res["fixtures"])
            res["fixtures"]["restore_error"] = ferr
            try:
                write_json_file(os.path.join(side_dir, "fixtures.json"),
                                res["fixtures"])
            except OSError:
                pass
            if ferr:
                msg = "窗口夹具还原失败：%s" % ferr
                res["error"] = (res["error"] + "；" + msg) if res["error"] else msg
                if res["rc"] == 0:
                    res["rc"] = 2
        # 8c) 还原等级夹具（DB）：停栈后把 Level 写回并**重读**校验；失败该侧 rc=2。
        if res.get("level"):
            lerr = restore_level_fixture(runner, level_state, env)
            res["level"]["restore_error"] = lerr
            try:
                write_json_file(os.path.join(side_dir, "level.json"), res["level"])
            except OSError:
                pass
            if lerr:
                msg = "等级夹具还原失败：%s" % lerr
                res["error"] = (res["error"] + "；" + msg) if res["error"] else msg
                if res["rc"] == 0:
                    res["rc"] = 2
        # 8d) 还原背包替换/丢弃夹具（DB）：停栈后按快照整行写回并重读；失败该侧 rc=2。
        if res.get("inventory_fixture"):
            ierr = restore_inventory_fixture(runner, inventory_fixture_state, env)
            res["inventory_fixture"]["restore_error"] = ierr
            try:
                write_json_file(os.path.join(side_dir, "inventory_fixture.json"),
                                res["inventory_fixture"])
            except OSError:
                pass
            if ierr:
                msg = "背包替换/丢弃夹具还原失败：%s" % ierr
                res["error"] = (res["error"] + "；" + msg) if res["error"] else msg
                if res["rc"] == 0:
                    res["rc"] = 2
        # 8e) RNG 台账体积控制（卡 openmir2-fixture-monsters-prepend ③）：栈已停 ⇒ 台账
        # 不再增长；超限则只留 head（INJECT_OK/SEED_PROBE 在最前）+ 记录原始 size/行数/sha256。
        if res.get("rng_log") and rng_ledger_trim_allowed(stack_attempted,
                                                          res.get("stop_rc")):
            trim = trim_rng_ledger(res["rng_log"])
            res["rng_log_trim"] = trim
            if trim.get("trimmed"):
                runner.log("[%s] RNG 台账超限已裁剪：%d 行/%.1f MiB -> %d 行/%.1f MiB"
                           "（原始 sha256=%s）"
                           % (side, trim["original_lines"],
                              trim["original_bytes"] / 1048576.0,
                              trim["kept_lines"], trim["kept_bytes"] / 1048576.0,
                              trim["original_sha256"][:16]))


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


def _postrun_sha(path):
    try:
        with open(path, "rb") as f:
            return hashlib.sha256(f.read()).hexdigest()
    except OSError:
        return None


def _postrun_tree(root):
    out = {}
    for base, _dirs, files in os.walk(root):
        for fn in files:
            fp = os.path.join(base, fn)
            rel = os.path.relpath(fp, root).replace(os.sep, "/")
            sha = _postrun_sha(fp)
            if sha:
                out[rel] = sha
    return out


def postrun_symmetry(prof):
    """**跑后**对称性复核（restore 之后、run.json 落盘之前）。

    C# 的 `CastleMgr.Save()`/`SaveCastleList()` 会在运行期按内存状态重写
    `Castle/List.txt` 与 `Castle/<n>/SabukW.txt`（与 600s 门同一机制）⇒ 跑后漂移是**预期行为**，
    但必须在下一轮前还原/对齐。这里只**标注**（不 fail 掉本轮 parity），返回差异列表：

      [{"file": "Castle/List.txt", "sha256": {"csharp": "...", "rust": "..."}}, ...]
    """
    sides = prof.get("sides") or {}
    expanded = []
    for side in SIDES:
        root = (sides.get(side) or {}).get("envir_root")
        if not root:
            continue
        root = str(root)
        expanded.append((side, root))
        if side == "csharp":
            shadow = os.path.join(os.path.dirname(os.path.abspath(root)),
                                  "M2GameSvr-hook")
            if os.path.isdir(shadow):
                expanded.append(("csharp-hook", shadow))
    if len(expanded) < 2:
        return []
    diffs = []

    def cmp_rel(rel):
        shas = {}
        for label, root in expanded:
            path = os.path.join(root, *rel)
            sha = _postrun_sha(path)
            if sha:
                shas[label] = sha
        if len(set(shas.values())) > 1:
            diffs.append({"file": "/".join(rel), "sha256": shas})

    cmp_rel(("Castle", "List.txt"))
    castle_names = set()
    for _label, root in expanded:
        cdir = os.path.join(root, "Castle")
        if not os.path.isdir(cdir):
            continue
        for name in os.listdir(cdir):
            if os.path.isfile(os.path.join(cdir, name, "SabukW.txt")):
                castle_names.add(name)
    for name in sorted(castle_names):
        cmp_rel(("Castle", name, "SabukW.txt"))
    base = None
    for label, root in expanded:
        edir = os.path.join(root, "Envir")
        if not os.path.isdir(edir):
            continue
        tree = _postrun_tree(edir)
        if base is None:
            base = (label, tree)
            continue
        for rel in sorted(set(base[1]) | set(tree)):
            a, b = base[1].get(rel), tree.get(rel)
            if a != b:
                diffs.append({"file": "Envir/" + rel,
                              "sha256": {base[0]: a, label: b}})
    return diffs[:200]


def execute(cfg):
    """cfg: dict(card, scenario, out, profile, password, post_comment, dry_run,
                 allow_production_db, rules, timeout_ms, ...) -> summary"""
    dry_run = bool(cfg.get("dry_run"))
    # --var 透传（卡 openmir2-card-window-var-passthrough）：**加载期**解析并校验——非法形式
    # 在建 out / 起栈 / 跑 discovery / 写 report 之前就 rc=2 点名（fail-closed）。
    user_vars = parse_var_args(cfg.get("vars"))
    raw_profile = cfg.get("profile")
    prof = load_profile(raw_profile if raw_profile is not None else "alt")
    prof = apply_overrides(prof, cfg)
    # 报错里点名的 profile：给了内建名/路径就原样用；inline dict（selftest/嵌入）用其 name。
    profile_label = (raw_profile if isinstance(raw_profile, str) and raw_profile
                     else (prof.get("name") or "<inline>"))
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
    blocked = None
    inventory = None
    assets = None
    requires_decl = {"name": "", "requires": []}
    if scenario and os.path.isfile(scenario):
        required = scenario_required(scenario)
        computed = scenario_computed(scenario)
        placement = scenario_placement(scenario)
        blocked = scenario_blocked(scenario)
        inventory = scenario_inventory(scenario)
        assets = scenario_assets(scenario)
        requires_decl = scenario_requires(scenario)
    discover = bool(cfg.get("discover"))
    discovery_scenario = os.path.abspath(
        cfg.get("discovery_scenario") or DEFAULT_DISCOVERY_SCENARIO)
    needs_discovery = discover or bool(required)
    discovery_cfg = {"needed": needs_discovery, "forced": discover,
                     "scenario": discovery_scenario, "required": required,
                     "computed": computed,
                     "frames": cfg.get("discovery_frames")}

    _ss_decl = summary_save_settle_decl(prof)   # 存档落定等待：每侧生效配置（非法则 None）
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
        "magic": {
            "enabled": any(profile_magic(prof, s) for s in SIDES),
            "declared": profile_magic(prof),
            "sides": {},
            "note": ("profile.magic => 每侧 restore（与预置位）之后、起栈之前把该侧测试库 "
                     "characters_magic 写到位；完成不了 rc=2（不让 probe 超时）"
                     if profile_magic(prof) else None),
        },
        "inventory": {
            "enabled": bool(inventory),
            "declared": inventory,
            "sides": {},
            "note": ("场景顶层 inventory => 每侧 restore 之后、起栈之前把该侧测试库 "
                     "characters_bagitem 写到位；完成不了 rc=2（不让 probe 超时）"
                     if inventory else None),
        },
        "fixtures": {
            "enabled": any(profile_fixtures(prof, s) for s in SIDES),
            "declared": profile_fixtures(prof),
            "sides": {},
            "note": ("profile.fixtures => 每侧 restore 之后、起栈之前应用声明式夹具"
                     "（monsters 单点刷怪 + 配置覆盖 + 城堡前置；对基线 + hook 影子各写一份；"
                     "写前备份 sha256、写后读回）；停栈后按 sha256 还原；未知字段/键/越界值"
                     "加载期 fail-closed"
                     if any(profile_fixtures(prof, s) for s in SIDES) else None),
        },
        "save_settle": {
            "enabled": bool(_ss_decl and any(c.get("enabled") for c in _ss_decl.values())),
            "declared": _ss_decl,
            "sides": {},
            "note": ("probe 之后、DB 快照之前等两侧「存档落定」：轮询 characters.LASTUPDATE "
                     "+ 行数 + characters_ablity.ModifyTime（第二见证）指纹，静默 >= quiet_ms "
                     "才算落定；窗口内不落定，或 require_changes（默认 true）下窗口内一次变化都"
                     "没观测到 ⇒ 该侧 rc=2 save_unsettled、不拍库。**静默 != 存档已触发**"
                     "（卡 openmir2-harness-save-time-symmetry）"),
        },
        "assets": {
            "enabled": bool(assets),
            "declared": assets,
            "sides": {},
            "note": ("场景顶层 assets => 每侧 restore 之后、起栈之前 UPDATE 该侧测试库 "
                     "characters 的白名单资产列到目标值（首版 Gold）；完成不了 rc=2"
                     "（不让 probe 超时）"
                     if assets else None),
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
    if user_vars:
        # 只在真给了 --var 时落该字段：不传时产物逐字节不变。
        summary["probe_vars"] = list(user_vars)

    if scenario and os.path.isfile(scenario):
        summary["scenario_sha256"] = sha256_file(scenario)

    # BLOCKED 场景硬门（卡 openmir2-window-blocked-scenario-hard-gate）。
    # 位置刻意在「建 out 目录 / 起栈 / 跑 discovery / 写 report」**之前**，--dry-run 与真跑
    # 走同一条路径：见到 blocked 就停（rc=2，与「端口被占」同一失败等级），连命令序列都不预览
    # —— 免得 dry-run 绿了让人误以为可以真跑。
    if blocked:
        summary["blocked"] = {"reason": blocked["reason"],
                              "missing_fixture": blocked["missing_fixture"] or ""}
        summary["notes"].append("scenario is BLOCKED: %s" % blocked["reason"])
        summary["notes"].append("missing_fixture: %s"
                                % (blocked["missing_fixture"] or "(场景未声明)"))
        summary["verdict"] = "BLOCKED"
        summary["exit_code"] = 2
        summary["finished_at"] = iso_now()
        summary["duration_s"] = _fmt_duration(time.monotonic() - t0)
        return summary

    # 夹具前置（卡 `openmir2-scenario-fixture-precondition`）：场景声明了 `requires` 就按**生效
    # profile** 逐侧核（含 `sides.<side>` 覆盖）。位置与 BLOCKED 硬门同级——在建 out / 起栈 /
    # 跑 discovery / 写 report **之前**，`--dry-run` 与真跑走同一条路径，缺段一律 rc=2 并点名；
    # 没有 `requires` 的场景在这里是 no-op（行为逐字节不变）。
    validate_scenario_requires(scenario, requires_decl, prof, profile_label)

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
            ctx["rng_log"] = bool(cfg.get("rng_log"))
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
            if not side_failed and inventory:
                iprev, ifailed = preview_inventory(runner, side, prof, sc, inventory, ctx)
                summary.setdefault("inventory", {}).setdefault("sides", {})[side] = iprev
                if ifailed:
                    runner.log("[%s] 背包夹具不可行 ⇒ 真跑 rc=2（不起栈、不跑场景）：%s"
                               % (side, iprev.get("error")))
                    preview_failed = True
                    side_failed = True
            if not side_failed:
                # 卡 `openmir2-scenario-fixture-migration`：把 profile 的
                # `fixtures.inventory_replace`/`inventory_drop` 预览接进**主 CLI dry-run**
                # （此前只有 selftest 调用该 dry-run 分支，主 dry-run 一行不打 ⇒ 误以为场景写错）。
                try:
                    fdecl = profile_fixtures(prof, side) or profile_fixtures(prof)
                except ConfigError as e:
                    runner.log("[%s] 窗口夹具声明非法 ⇒ 真跑 rc=2：%s" % (side, e))
                    preview_failed = True
                    side_failed = True
                    fdecl = None
                if fdecl and (fdecl.get("inventory_replace") or fdecl.get("inventory_drop")):
                    frev = run_inventory_fixture(runner, side, prof, sc, fdecl, {}, None)
                    summary.setdefault("fixtures", {}).setdefault("sides", {})[side] = frev
                    if frev.get("rc") != 0:
                        runner.log("[%s] 背包替换夹具不可行 ⇒ 真跑 rc=2：%s"
                                   % (side, frev.get("error")))
                        preview_failed = True
                        side_failed = True
            if not side_failed and assets:
                aprev, afailed = preview_assets(runner, side, prof, sc, assets, ctx)
                summary.setdefault("assets", {}).setdefault("sides", {})[side] = aprev
                if afailed:
                    runner.log("[%s] 资产声明通道不可行 ⇒ 真跑 rc=2（不起栈、不跑场景）：%s"
                               % (side, aprev.get("error")))
                    preview_failed = True
                    side_failed = True
            if not side_failed:
                try:
                    magic_side = profile_magic(prof, side)
                except ConfigError as e:
                    runner.log("[%s] 魔法夹具声明非法 ⇒ 真跑 rc=2：%s" % (side, e))
                    summary["notes"].append("魔法夹具声明非法：%s" % e)
                    preview_failed = True
                    side_failed = True
                    magic_side = None
                if magic_side:
                    mprev, mfailed = preview_magic(runner, side, prof, sc, magic_side,
                                                   ctx)
                    summary["magic"]["sides"][side] = mprev
                    if mfailed:
                        runner.log("[%s] 魔法夹具不可行 ⇒ 真跑 rc=2（不起栈、不跑场景）：%s"
                                   % (side, mprev.get("error")))
                        preview_failed = True
                        side_failed = True
            if not side_failed:
                try:
                    fixture_side = profile_fixtures(prof, side)
                except ConfigError as e:
                    runner.log("[%s] 窗口夹具声明非法 ⇒ 真跑 rc=2：%s" % (side, e))
                    summary["notes"].append("窗口夹具声明非法：%s" % e)
                    preview_failed = True
                    side_failed = True
                    fixture_side = None
                if fixture_side:
                    fprev, ffailed = preview_fixtures(runner, side, prof, sc,
                                                      fixture_side, ctx)
                    summary["fixtures"]["sides"][side] = fprev
                    if ffailed:
                        runner.log("[%s] 窗口夹具不可行 ⇒ 真跑 rc=2（不起栈、不跑场景）：%s"
                                   % (side, fprev.get("error")))
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
                try:
                    settle_side = profile_save_settle(prof, side)
                except ConfigError as e:
                    runner.log("[%s] 存档落定等待配置非法 ⇒ 真跑 rc=2：%s" % (side, e))
                    summary["notes"].append("存档落定等待配置非法：%s" % e)
                    preview_failed = True
                    side_failed = True
                    settle_side = None
                if settle_side and settle_side.get("enabled"):
                    runner.log("[%s] 存档落定等待：window=%dms quiet=%dms poll=%dms"
                               "（probe 后、dbsnap 前；超时 => save_unsettled rc=2）"
                               % (side, settle_side["window_ms"], settle_side["quiet_ms"],
                                  settle_side["poll_ms"]))
            if not side_failed:
                runner.run(side, "probe",
                           build_probe_argv(sc, ctx, prof, password,
                                            vars_argv=list(vars_argv)
                                            + list(user_vars or [])))
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
        ctx["rng_log"] = bool(cfg.get("rng_log"))
        try:
            res = run_side(runner, side, prof, ctx, password, placement=placement,
                           inventory=inventory, assets=assets, user_vars=user_vars)
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
        if res.get("magic"):
            summary["magic"]["sides"][side] = res["magic"]
        if res.get("inventory"):
            # 卡 openmir2-card-window-teeth-mg7：`res["inventory"]` 不能只写不读——
            # 把该侧证据并进 summary。**rc 判定不在这里**：`run_side` 的 `rc` 默认 2，
            # 背包夹具失败时它在写 `<side>/inventory.json` 之后立刻 `return`（`rc=2` + `error`），
            # 所以本层再兜一次 rc 是**死分支**（卡 `openmir2-scenario-fixture-precondition`
            # 收掉的 minor①）；真正起作用的判据在 `run_side` 的 `if ires.get("rc") != 0`。
            summary["inventory"]["sides"][side] = res["inventory"]
        if res.get("assets"):
            summary["assets"]["sides"][side] = res["assets"]
        if res.get("fixtures"):
            summary["fixtures"]["sides"][side] = res["fixtures"]
        if res.get("save_settle"):
            # review `b48197f6` F2①：窗口报告必须能直接看出每侧「有没有真的等到存档」
            # （changes/settled/elapsed_ms），否则 DB 面 diff 无从判断是真差异还是快照早拍。
            _ss = res["save_settle"]
            summary["save_settle"]["sides"][side] = {
                "changes": _ss.get("changes"), "settled": _ss.get("settled"),
                "elapsed_ms": _ss.get("elapsed_ms"), "action": _ss.get("action"),
                "rc": _ss.get("rc"), "error": _ss.get("error")}
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

    # 跑后对称性复核（卡 openmir2-postrun-symmetry-drift）：C# 运行期会重写
    # Castle/List.txt 与 SabukW ⇒ 只标注、绝不改变本轮 parity 结论。
    try:
        drift = postrun_symmetry(prof)
    except Exception as e:  # 防御：复核自身出错也不能 fail 掉本轮
        drift = []
        summary["notes"].append("跑后对称性复核失败（已忽略）：%s: %s"
                                % (type(e).__name__, e))
    summary["post_run_symmetry_drift"] = drift
    if drift:
        summary["notes"].append(
            "跑后对称性漂移 %d 项（预期行为，下一轮前须还原/对齐）：%s"
            % (len(drift), ", ".join(d.get("file") or "?" for d in drift[:8])))

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
        if "/* magic */" in sql_text:
            # 魔法夹具桩（卡 openmir2-window-magic-fixture）：状态落 `<log>.magic-<side>`。
            mstate = log + ".magic-" + side
            ms = {"char_id": 6228, "char_count": 1, "deleted": 0,
                  "rows": {}, "fp": "fp-magic-before"}
            init = os.environ.get("STUB_MAGIC_INIT")
            if init:
                ms["rows"] = json.loads(init)
            if os.path.exists(mstate):
                try:
                    with open(mstate, encoding="utf-8") as f:
                        ms = json.load(f)
                except (OSError, ValueError):
                    pass
            ms["char_count"] = int(os.environ.get("STUB_MAGIC_CHAR_COUNT",
                                                  ms.get("char_count", 1)))
            ms["deleted"] = int(os.environ.get("STUB_MAGIC_DELETED", ms.get("deleted", 0)))

            def _msave():
                with open(mstate, "w", encoding="utf-8") as f:
                    json.dump(ms, f, sort_keys=True)

            # 查询对象是 `characters`（不是 `characters_magic`）：必须回 characters
            # 表列，run_magic 要用 Id/ChrName/LoginID/Deleted。留一行日志标记，
            # 供 selftest 证明本分支真的被走到（卡 openmir2-magic-fixture-selftest-gaps）。
            if "SHOW COLUMNS" in sql_text:
                with open(log, "a", encoding="utf-8") as lf:
                    lf.write("magic-columns %s\n" % side)
                print("\n".join("%s\tint(11)\tYES\t\tNULL\t" % c for c in
                                ["Id", "ServerIndex", "LoginID", "ChrName", "MapName",
                                 "CX", "CY", "Level", "HomeMap", "Deleted"]))
                return 0
            if "INSERT INTO" in sql_text or "UPDATE " in sql_text:
                if os.environ.get("STUB_MAGIC_NOWRITE") != side:
                    mi = _re.search(r"VALUES \((\d+), (\d+), (\d+), '([^']*)', (\d+)\)",
                                    sql_text)
                    mu = _re.search(r"`PlayerId` = (\d+) AND `MagicId` = (\d+)", sql_text)
                    mset = _re.search(
                        r"`Level` = (\d+), `UseKey` = '([^']*)', `CurrTrain` = (\d+)",
                        sql_text)
                    if mi:
                        ms["rows"][mi.group(2)] = {"level": int(mi.group(3)),
                                                   "use_key": mi.group(4),
                                                   "curr_train": int(mi.group(5))}
                    elif mu and mset:
                        ms["rows"][mu.group(2)] = {"level": int(mset.group(1)),
                                                   "use_key": mset.group(2),
                                                   "curr_train": int(mset.group(3))}
                    if os.environ.get("STUB_MAGIC_CORRUPT") == side:
                        ms["fp"] = "fp-magic-corrupt"
                _msave()
                print("1")
                return 0
            if "GROUP BY" in sql_text:
                if os.environ.get("STUB_MAGIC_DUP") == side:
                    print("1	2")
                return 0
            if "NOT IN" in sql_text:
                print("%s	%s" % (ms.get("other_count", 0),
                                   ms.get("fp", "fp-magic-before")))
                return 0
            if "IN (" in sql_text:
                mm = _re.search(r"IN \(([0-9, ]+)\)", sql_text)
                ids = [x.strip() for x in mm.group(1).split(",")] if mm else []
                for mid in ids:
                    row = ms["rows"].get(mid)
                    if row:
                        print("%s	%s	%s	%s" % (mid, row["level"], row["use_key"],
                                                     row["curr_train"]))
                return 0
            if ms["char_count"] <= 0:
                print("0	0	0")
            else:
                print("%d	%d	%d" % (ms["char_count"], ms["char_id"], ms["deleted"]))
            return 0
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
        # 合成夹具要**反映真实形态**（卡 `openmir2-db-makeindex-mask` 的 post-merge 修复）：
        # 真机 `characters_bagitem` 的占用槽 `MakeIndex` 非 0；旧夹具写 0 是**空壳行**，
        # 会让已登记掩码列的 `nonzero_both_sides` 判红（「声明了掩码却证明不了这列被填过」）
        # ——那是**夹具不真实**，不是判据过严。行数保持 1（其余自测按单行夹具断言行数/恢复）。
        payload = ("1\t1\t0\t5001\t0\t0\t0\n").encode("utf-8")
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
        # 卡 openmir2-harness-save-time-symmetry：自测用**快档**落定参数（默认 5000/800/200
        # 会让每个走 run_side 的用例真等 ~0.8s）；语义仍由 SETTLE1..16 覆盖。
        "save_settle": {"window_ms": 300, "quiet_ms": 100, "poll_ms": 50,
                        "require_changes": False},
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


def _selftest_inventory(tmp):
    """卡 openmir2-window-inventory-fixture：声明解析/校验/指纹/行数（4 用例 + 2 牙）。"""
    import json as _json
    out = []
    d = os.path.join(tmp, "inv")
    os.makedirs(d, exist_ok=True)
    path = os.path.join(d, "inv-scenario.json")
    good = {"inventory": [
        {"index": 100, "make_index": 5001, "position": 3, "std_mode": 0, "ani_count": 0},
        {"index": 101, "make_index": 5002, "position": 7, "dura": 1000, "dura_max": 2000}]}
    with open(path, "w", encoding="utf-8") as f:
        _json.dump(good, f)
    items = scenario_inventory(path)
    out.append((len(items) == 2 and items[0]["index"] == 100 and items[0]["position"] == 3
                and items[1]["dura"] == 1000, "① 合法声明 => 解析出 2 件且字段透传"))
    # ② 两侧 MakeIndex 相同 => 通过；不同 => 抛错（牙②）
    try:
        compare_inventory_make_index(items, "other", list(items))
        same_ok = True
    except ConfigError:
        same_ok = False
    out.append((same_ok, "② 两侧 MakeIndex 相同 => 校验通过"))
    # ③ 非法声明 => ConfigError 点名（槽位越界 / 重复槽位 / 缺键 / 未知键）
    bad_cases = [
        ({"inventory": [{"index": 1, "make_index": 1, "position": 46}]}, "position"),
        ({"inventory": [{"index": 1, "make_index": 1, "position": 1},
                        {"index": 2, "make_index": 2, "position": 1}]}, "重复"),
        ({"inventory": [{"make_index": 1, "position": 1}]}, "缺必填"),
        ({"inventory": [{"index": 1, "make_index": 1, "position": 1, "zzz": 1}]}, "未知键"),
    ]
    bad_ok = True
    for body, needle in bad_cases:
        with open(path, "w", encoding="utf-8") as f:
            _json.dump(body, f)
        try:
            scenario_inventory(path)
        except ConfigError as ex:
            if needle not in str(ex):
                bad_ok = False
        else:
            bad_ok = False
    out.append((bad_ok, "③ 非法声明（越界/重复/缺键/未知键）=> ConfigError 且点名"))
    # ④ 指纹列 = SHOW COLUMNS 去掉「声明列」（只改需要列的证据面）
    cols = ["Id", "PlayerId", "Position", "MakeIndex", "StdIndex",
            "Dura", "DuraMax", "Desc0", "Desc1"]
    fp_sql = inventory_fingerprint_sql("db", 6228, cols)
    out.append(("`Desc0`" in fp_sql and "`Desc1`" in fp_sql and "`MakeIndex`" not in fp_sql,
                "④ 指纹只含未声明列（Desc*），不含 MakeIndex/Position"))
    # ④b 实表全部列都在声明列集内（DESCRIBE mir2_db.characters_bagitem 就是这 7 列）⇒
    #    指纹必须退化为只比行数（`CONCAT_WS('')` 单参调用 MySQL 报错——真库实测踩中）。
    real_cols = ["Id", "PlayerId", "Position", "MakeIndex", "StdIndex", "Dura", "DuraMax"]
    fp_real = inventory_fingerprint_sql("db", 6228, real_cols)
    out.append(("CONCAT_WS" not in fp_real and fp_real.count("SELECT COUNT(*)") == 1,
                "④b 实表无余列 ⇒ 指纹退化为行数（不再拼 CONCAT_WS）"))
    # 牙①：跳过预置（写后行数不足）=> 必须报错
    try:
        check_inventory_rows(items, 1)
        tooth_ok = False
    except ConfigError:
        tooth_ok = True
    out.append((tooth_ok, "牙①：写后行数 < 声明件数 => 报错（跳过预置即红）"))
    # 牙②：MakeIndex 两侧不同 => 必须报错并点名
    shuffled = [dict(it) for it in items]
    shuffled[0]["make_index"] = 9999
    try:
        compare_inventory_make_index(items, "other", shuffled)
        tooth2_ok = False
    except ConfigError as ex:
        tooth2_ok = "MakeIndex" in str(ex)
    out.append((tooth2_ok, "牙②：两侧 MakeIndex 不同 => 报错并点名"))

    # ---- 生产接线 / 失败面（review `c866f358` I-3 / M-1）：用假 runner 跑 run_inventory 本体 ----
    class _FakeRunner:
        def __init__(self, std_mode="0", update_rc=0, verify_fp="-",
                     occupied=0, before_count=0, after_count=0,
                     tpl_missing=False, rows_written=1):
            self.std_mode = std_mode
            self.update_rc = update_rc
            self.verify_fp = verify_fp
            self.occupied = occupied
            self.before_count = before_count
            self.after_count = after_count
            self.tpl_missing = tpl_missing
            self.rows_written = rows_written

        def run(self, side, tag, argv, env=None, timeout=120, stdin_text=None, capture=True):
            if tag == "inventory.columns":
                return 0, ("Id\nPlayerId\nPosition\nMakeIndex\nStdIndex"
                           "\nDura\nDuraMax\nDesc0\n"), ""
            if tag == "inventory.character.columns":
                return 0, "Id\nChrName\nLoginID\nDeleted\n", ""
            if tag == "inventory.character":
                return 0, "1\t6228\t0\n", ""
            if tag == "inventory.occupancy":
                return 0, "%d\n" % self.occupied, ""
            if tag == "inventory.template":
                if self.tpl_missing:
                    return 0, "", ""
                return 0, "%s\t0\t0\n" % self.std_mode, ""
            if tag == "inventory.read":
                return 0, "%d\t-\n" % self.before_count, ""
            if tag == "inventory.update":
                return self.update_rc, "", ("boom" if self.update_rc else "")
            if tag == "inventory.verify":
                return 0, "%d\t%s\n" % (self.after_count, self.verify_fp), ""
            if tag == "inventory.verify_rows":
                return 0, "%d\n" % self.rows_written, ""
            return 1, "", "unexpected tag %s" % tag

    class _OccupancyRunner(_FakeRunner):
        """按 SQL 的 WHERE 真算占用行数（其余 tag 全部走 `_FakeRunner`）。

        `rows` = [(Position, StdIndex, MakeIndex)]。SQL 里带 `StdIndex/MakeIndex <> 0` 谓词时
        只数非空行；**谓词一旦被删掉**（变异）就退化成「数所有行」= 修复前的 buggy 语义，
        ① 立刻红 —— 这就是这两颗牙的咬合点。
        """

        def __init__(self, rows):
            _FakeRunner.__init__(self)
            self.rows = rows

        def run(self, side, tag, argv, env=None, timeout=120, stdin_text=None,
                capture=True):
            if tag != "inventory.occupancy":
                return _FakeRunner.run(self, side, tag, argv, env=env, timeout=timeout,
                                       stdin_text=stdin_text, capture=capture)
            text = stdin_text or ""
            m = re.search(r"`Position` IN \(([^)]*)\)", text)
            pos = {int(x) for x in m.group(1).split(",")} if m else set()
            # **真的算 WHERE**：把 SQL 里最后一个 `AND (… )` 谓词抠出来，去掉反引号、
            # `<>` 换成 `!=` 后求值。这样「整段谓词删掉」与「OR→AND」两种变异都会改变
            # 结果——牙才咬得住 SQL 本身，而不是由假 runner 另写一套语义（那是装饰）。
            # 卡 `openmir2-cardwindow-occupancy-parser-safety`：**不再裸 rfind**——按
            # `StdIndex` 认组 + 括号配平（见 `_pick_occupancy_pred`）；找不到 ⇒ None ⇒
            # 退化成「数所有行」⇒ ⑲ 立刻红，不静默。
            cand = _pick_occupancy_pred(text)
            expr = None
            if cand is not None:
                # SQL 的 `=` 要译成 `==`：**不译的话 `eval` 会抛 SyntaxError、被下面
                # 的 except 吞成 keep=False ⇒ 谓词恒假 ⇒ 空槽判定反过来把真物品当空槽**。
                # 用字母数字边界避免把 `<=`/`>=`/`!=` 译坏。
                expr = (cand.replace("`", "").replace("<>", "!=")
                        .replace(" OR ", " or ").replace(" AND ", " and "))
                expr = re.sub(r"(?<![<>=!])=(?!=)", "==", expr)
            n = 0
            for (p, std, mk) in self.rows:
                if p not in pos:
                    continue
                if expr is not None:
                    try:
                        keep = bool(eval(expr, {"__builtins__": {}},
                                        {"StdIndex": std, "MakeIndex": mk}))
                    except Exception:
                        keep = False
                    if not keep:
                        continue
                n += 1
            return 0, "%d\n" % n, ""

    prof = {"char_name": "hero", "account": "ua",
            "sides": {"csharp": {"db": {"characters_db": "mir2_test_db"}}}}
    sc = prof["sides"]["csharp"]
    one = [{"index": 100, "make_index": 5001, "position": 3, "std_mode": 0,
            "dura": 0, "dura_max": 0}]

    orig_pair = globals()["_check_db_pairing"]
    orig_root = globals()["placement_envir_root"]
    globals()["_check_db_pairing"] = lambda *a, **k: None
    globals()["placement_envir_root"] = lambda *a, **k: tmp
    try:
        ok = run_inventory(_FakeRunner(), "csharp", prof, sc, one, {})
        out.append((ok["rc"] == 0 and ok["sql"].get("update") and ok["db_pairing"] is None,
                    "⑤ 生产路径：假 runner 走通 run_inventory（rc=0、有 UPDATE SQL）"))
        bad_std = run_inventory(_FakeRunner(std_mode="31"), "csharp", prof, sc, one, {})
        out.append((bad_std["rc"] == 2 and "StdMode" in (bad_std["error"] or ""),
                    "⑥ StdMode 与声明不符 => rc=2 且点名"))
        bad_upd = run_inventory(_FakeRunner(update_rc=1), "csharp", prof, sc, one, {})
        out.append((bad_upd["rc"] == 2 and "inventory.update" in (bad_upd["error"] or ""),
                    "⑦ UPDATE 执行失败 => rc=2 且点名"))
        bad_fp = run_inventory(_FakeRunner(verify_fp="deadbeef"), "csharp", prof, sc, one, {})
        out.append((bad_fp["rc"] == 2 and bad_fp["altered_columns"],
                    "⑧ 其它槽位指纹变了 => rc=2 且记 altered_columns"))
        bad_occ = run_inventory(_FakeRunner(occupied=1), "csharp", prof, sc, one, {})
        out.append((bad_occ["rc"] == 2 and "已有" in (bad_occ["error"] or ""),
                    "⑪ 声明槽位已被占用 => rc=2（%r）" % (bad_occ["error"] or "")[:90]))
        bad_cnt = run_inventory(_FakeRunner(after_count=1), "csharp", prof, sc, one, {})
        out.append((bad_cnt["rc"] == 2 and "行数变了" in (bad_cnt["error"] or ""),
                    "⑫ 其它槽位行数变了 => rc=2（%r）" % (bad_cnt["error"] or "")[:90]))
        # ⑬ 物品模板不存在（卡 openmir2-card-window-teeth-mg7）：`if not tpl:` 必须真的拦下
        try:
            bad_tpl = run_inventory(_FakeRunner(tpl_missing=True), "csharp", prof, sc,
                                    one, {})
            tpl_ok = bad_tpl["rc"] == 2 and "物品模板" in (bad_tpl["error"] or "")
        except Exception:
            # 变异（if not tpl: → if False:）会让 tpl[0] 抛 IndexError：干净判红，不崩 selftest。
            tpl_ok = False
        out.append((tpl_ok, "⑬ 物品模板不存在 => rc=2 且点名 mir2_data.stditems"))
        # ⑮ check_inventory_rows 的调用点：写后行数不足必须从 run_inventory 本体冒出来
        two = [dict(one[0]), dict(one[0], make_index=5002, position=7)]
        short = run_inventory(_FakeRunner(rows_written=1), "csharp", prof, sc, two, {})
        out.append((short["rc"] == 2 and "写后行数" in (short["error"] or ""),
                    "⑮ 写后行数 < 声明件数 => rc=2（%r）" % (short["error"] or "")[:90]))
        # ①②（reviewer-21 `3dc30ae5` 返工）：占用谓词必须是「有物品」而不是「有行」。
        # 假 runner 按 SQL 的 WHERE 真算行数，所以把谓词删掉（变异）⇒ ① 会红。
        empty_slot = run_inventory(_OccupancyRunner([(3, 0, 0)]), "csharp", prof, sc, one, {})
        out.append((empty_slot["rc"] == 0,
                    "⑲ 空占位行（StdIndex=0 且 MakeIndex=0）不算占用 => rc=0（%r）"
                    % (empty_slot["error"] or "")[:80]))
        real_slot = run_inventory(_OccupancyRunner([(3, 207, 5001)]), "csharp", prof, sc, one, {})
        out.append((real_slot["rc"] == 2 and "已有" in (real_slot["error"] or ""),
                    "⑳ 真有物品的行（StdIndex/MakeIndex 非 0）仍算占用 => rc=2（%r）"
                    % (real_slot["error"] or "")[:80]))
        # ⑴：谓词必须是 **OR**——老物品行可能 `StdIndex>0` 但 `MakeIndex=0`（历史行/未写合成键），
        # 写成 AND 会把它当空槽（那就真的会覆盖已有物品）。
        legacy_slot = run_inventory(_OccupancyRunner([(3, 207, 0)]), "csharp", prof, sc, one, {})
        out.append((legacy_slot["rc"] == 2 and "已有" in (legacy_slot["error"] or ""),
                    "㉑ 只有 StdIndex 非 0（老物品行）也算占用 => rc=2（%r）"
                    % (legacy_slot["error"] or "")[:80]))
        # ㉒a/b（卡 `openmir2-cardwindow-occupancy-parser-safety` A2）：**多 AND 组**的合成 SQL。
        # 旧实现的裸 `rfind("AND (")` 会取到**最后**那组（就不是占用谓词）。
        base_sql = ("SELECT COUNT(*) FROM `db`.`characters_bagitem`\n"
                    " WHERE `PlayerId` = 6228 AND `Position` IN (3)\n"
                    "   AND (`StdIndex` <> 0 OR `MakeIndex` <> 0)")
        # a：白盒——解析器直接告诉我们它选了哪组
        multi_sql = base_sql + ("\n   AND (`Position` <> -1);\n")
        picked = _pick_occupancy_pred(multi_sql)
        out.append((picked is not None and "StdIndex" in picked and "MakeIndex" in picked
                    and "Position" not in picked,
                    "㉒a 多 AND 组时取的仍是占用谓词（不是最后一组）"))
        # b：行为面——尾那组改成 `StdIndex = 0`：裸 rfind 会取它、把空占位行算成占用
        trap_sql = base_sql + ("\n   AND (`StdIndex` = 0);\n")
        probe = _OccupancyRunner([(3, 0, 0)])
        rc_t, out_t, _ = probe.run("csharp", "inventory.occupancy", [], stdin_text=trap_sql)
        out.append((out_t.strip() == "0",
                    "㉒b 尾组是 `StdIndex = 0` 时仍不被当作谓词（空占位行不算占用）"))
        # c：找不到占用谓词（被删）=> 返回 None（调用方退化成数所有行），不静默
        out.append((_pick_occupancy_pred(base_sql.replace(
                        "   AND (`StdIndex` <> 0 OR `MakeIndex` <> 0)", "")) is None,
                    "㉒c 占用谓词被删 => 返回 None（上游退化成数所有行）"))
        # ㉒d：谓词里的 `=` 必须该被译成 `==`——**不译就是 `eval` 抛
        # SyntaxError 被 except 吞成 keep=False、谓词恒假**（空槽判定反过来把真物品当空槽）。
        eq_sql = ("SELECT COUNT(*) FROM `db`.`characters_bagitem`" + chr(10) +
                  " WHERE `PlayerId` = 6228 AND `Position` IN (3)" + chr(10) +
                  "   AND (`StdIndex` = 0 OR `MakeIndex` = 0)")
        probe_eq = _OccupancyRunner([(3, 0, 0)])
        rc_e, out_e, _ = probe_eq.run("csharp", "inventory.occupancy", [], stdin_text=eq_sql)
        out.append((out_e.strip() == "1",
                    "㉒d 谓词用 `=` 也能求值（译成 ==）：StdIndex=0 的行被算为占用"))
        # ㉓（卡 `openmir2-occupancy-none-behavior-tooth` A1）：**None ⇒ 上游 fail-closed 的直接行为牙**。
        # `_pick_occupancy_pred` 认不出占用谓词时返回 None，消费侧必须退化成「数所有行」
        # ——空占位行也被算成占用 ⇒ `run_inventory` 报 rc=2「已有 N 行」而拒绝；
        # **不得静默当成空表返回 0**——后者会把「谓词解析失配」变成「槽位空闲」⇒ 覆盖真物品。
        no_pred_sql = ("SELECT COUNT(*) FROM `db`.`characters_bagitem`" + chr(10) +
                       " WHERE `PlayerId` = 6228 AND `Position` IN (3)")
        probe_none = _OccupancyRunner([(3, 0, 0)])
        rc_n, out_n, _ = probe_none.run("csharp", "inventory.occupancy", [],
                                        stdin_text=no_pred_sql)
        out.append((_pick_occupancy_pred(no_pred_sql) is None and out_n.strip() == "1",
                    "㉓ 认不出谓词（None）⇒ 退化成数所有行（空占位行也算占用，fail-closed），不是静默 0"))
    finally:
        globals()["_check_db_pairing"] = orig_pair
        globals()["placement_envir_root"] = orig_root

    # ⑭ make_index<=0 门：0 会被 DBSvr 整行跳过，必须在解析期拒绝
    mk_path = os.path.join(d, "inv-make0.json")
    with open(mk_path, "w", encoding="utf-8") as f:
        _json.dump({"inventory": [{"index": 1, "make_index": 0, "position": 1}]}, f)
    try:
        scenario_inventory(mk_path)
        mk_ok = False
    except ConfigError as ex:
        mk_ok = "make_index" in str(ex)
    out.append((mk_ok, "⑭ make_index<=0 => ConfigError（DBSvr 会整行跳过）"))
    # ⑯ run_side 的背包夹具 rc 判定：失败必须 rc=2 并早返回（不起栈）
    class _SideRunner:
        def __init__(self):
            self.tags = []

        def run(self, side, tag, argv, env=None, timeout=120, stdin_text=None,
                capture=True):
            self.tags.append(tag)
            return 0

        def log(self, msg):
            pass

    orig_run_inventory = globals()["run_inventory"]
    globals()["run_inventory"] = lambda *a, **k: {
        "rc": 2, "error": "背包夹具失败：inventory: 物品模板 index=100 不存在"}
    sr_tags = []
    try:
        side_prof = {"sides": {"csharp": {
            "restore": ["true"], "stack": ["true"], "stop": ["true"],
            "probe": ["true"], "db": {"command": ["true"]},
            "ports": [], "ready_ports": []}}, "ready_timeout_s": 0.1}
        side_ctx = {"side_dir": os.path.join(d, "side16"),
                    "db_dir": os.path.join(d, "side16-db"),
                    "out": d, "scenario": path, "card": "selftest"}
        sr = _SideRunner()
        sres = run_side(sr, "csharp", side_prof, side_ctx, "pw",
                        inventory=[{"index": 100, "make_index": 5001, "position": 3}])
        sr_tags = sr.tags
        sr_ok = (sres.get("rc") == 2
                 and "背包夹具失败" in (sres.get("error") or "")
                 and "stack" not in sr.tags)
    except Exception:
        sr_ok = False
    finally:
        globals()["run_inventory"] = orig_run_inventory
    out.append((sr_ok, "⑯ run_side 背包夹具失败 => rc=2 且不起栈（tags=%r）" % (sr_tags,)))

    # ⑩ 库配对的真实判定（不 stub _check_db_pairing）：部署 `_r`、profile 拿 C# 库 => rc=2
    bad_root = os.path.join(tmp, "inv-bad-envir")
    os.makedirs(bad_root, exist_ok=True)
    with open(os.path.join(bad_root, "Server.conf"), "w", encoding="utf-8",
              newline="\n") as f:
        f.write("[DataBase]\nDbType=MySQL\n"
                "ConnctionString=server=127.0.0.1;uid=root;pwd=;"
                "database=mir2_test_data_r;\n")
    bad_sc = {"envir_root": bad_root, "db": {"characters_db": "mir2_test_db"}}
    pair_bad = run_inventory(_FakeRunner(), "csharp", prof, bad_sc, one, {})
    out.append((pair_bad["rc"] == 2 and "不配对" in (pair_bad["error"] or ""),
                "⑩ 库配对不匹配（部署 _r / profile C# 库）=> rc=2（%r）"
                % (pair_bad["error"] or "")[:90]))

    # ⑨ 生产接线顺序：run_side 必须调用 run_inventory，且排在 stack 之前（防旁路/挪位）
    import inspect
    side_src = inspect.getsource(run_side)
    out.append(("if inventory:" in side_src
                and "run_inventory(" in side_src
                and side_src.index("if inventory:") < side_src.index("run_inventory(")
                and side_src.index("run_inventory(") < side_src.index('"stack"'),
                "⑨ run_side 生产接线：if inventory: 走 run_inventory，且在起栈之前"))
    return out



def _selftest_assets(tmp):
    """卡 openmir2-window-gold-declaration-channel：资产声明通道
    （解析/白名单/值域/接线/真路径）。牙：① 非白名单列 => 红；② 负数/非数字 => 红；
    ③ dry-run 预览行出现目标值（正牙）。"""
    import json as _json
    out = []
    d = os.path.join(tmp, "assets")
    os.makedirs(d, exist_ok=True)
    path = os.path.join(d, "assets-scenario.json")

    def write_doc(body):
        with open(path, "w", encoding="utf-8") as f:
            _json.dump(body, f)

    write_doc({"assets": {"Gold": 10000, "note": "upgrade_retrieve 前置 Gold>=10000"}})
    decl = scenario_assets(path)
    out.append((decl == {"Gold": 10000, "note": "upgrade_retrieve 前置 Gold>=10000"},
                "ASSET1 合法声明（Gold=10000+note）=> 解析透传"))

    # 2026-10-25 卡 openmir2-window-kit-p0-unblock：GamePoint 进白名单（GAMEPOINT 窗口前置）
    write_doc({"assets": {"GamePoint": 9, "note": "GAMEPOINT 前置 >=9"}})
    gp = scenario_assets(path)
    out.append((gp.get("GamePoint") == 9,
                "ASSET1b GamePoint=9 => 解析透传（真爱-G002 @9hua 前置 CHECKGAMEPOINT > 8）"))
    # 牙1b（负向，load-bearing）：白名单退回旧值（只有 Gold）=> GamePoint 必须被拒，
    # 不许静默当 0/忽略——证明「白名单声明」本身是判据，不是装饰。
    orig_cols = globals()["ASSETS_COLUMNS"]
    globals()["ASSETS_COLUMNS"] = ("Gold",)
    try:
        write_doc({"assets": {"GamePoint": 9}})
        gp_rejected = False
        try:
            scenario_assets(path)
        except ConfigError as ex:
            gp_rejected = ("GamePoint" in str(ex) and "白名单" in str(ex))
    finally:
        globals()["ASSETS_COLUMNS"] = orig_cols
    out.append((gp_rejected,
                "ASSET 牙1b 白名单声明缺失（退回只有 Gold）=> GamePoint 声明被拒（不做静默 0）"))
    # 牙①：非白名单列 => ConfigError 点名列名与白名单（字符串列也不许按列名兜底混入）
    tooth1_ok = True
    for col in ("Level", "CreditPoint", "MasterName", "CREATEDATE"):
        write_doc({"assets": {"Gold": 10000, col: 1}})
        try:
            scenario_assets(path)
            tooth1_ok = False
        except ConfigError as ex:
            if "白名单" not in str(ex) or col not in str(ex):
                tooth1_ok = False
    out.append((tooth1_ok,
                "ASSET 牙1 非白名单列（Level/CreditPoint/MasterName/CREATEDATE）"
                "=> ConfigError 且点名白名单"))
    # 牙②：负数 / 非数字（字符串/浮点/bool/null）/ 超 int32
    bad_vals = [{"Gold": -1}, {"Gold": "10000"}, {"Gold": 10000.5},
                {"Gold": True}, {"Gold": None}, {"Gold": 2147483648}]
    tooth2_ok = True
    for body in bad_vals:
        write_doc({"assets": body})
        try:
            scenario_assets(path)
            tooth2_ok = False
        except ConfigError:
            pass
    out.append((tooth2_ok,
                "ASSET 牙2 负数/字符串/浮点/bool/null/超 int32 => ConfigError"))
    note_only_ok = True
    for body in ({"assets": {"note": "x"}}, {"assets": {}}):
        write_doc(body)
        try:
            scenario_assets(path)
            note_only_ok = False
        except ConfigError:
            pass
    out.append((note_only_ok, "ASSET3 只有 note/空对象 => ConfigError（无意义声明拒绝）"))

    rsql = _assets_row_sql("db", "winplay0", "winplay0",
                           ["Id", "ChrName", "LoginID", "Gold", "Level", "Deleted",
                            "CREATEDATE"], ["Gold"])
    out.append(("MIN(`Gold`)" in rsql and "CAST(`Gold`" not in rsql
                and "CAST(`Level`" in rsql and "CAST(`CREATEDATE`" in rsql
                and "FROM (SELECT `Id`, `Gold`," in rsql,
                "ASSET4 行 SQL：Gold 进现值槽不进指纹（且派生表透传 Gold——真库 1054 实锤），"
                "Level/CREATEDATE 进指纹"))
    rsql_all = _assets_row_sql("db", "c", "l", ["Gold"], ["Gold"])
    out.append(("CONCAT_WS" not in rsql_all,
                "ASSET4b 声明列覆盖全部列 => 指纹退化为常量（不拼 CONCAT_WS）"))

    # ---- 生产路径（假 runner 走 run_assets 本体；与 _selftest_inventory 同法）----
    class _FakeRunner:
        def __init__(self, before_gold=100, after_gold=10000, fp="a" * 32,
                     verify_fp=None, update_out="1\n", after_id=6228,
                     before_count=1, after_count=1, columns_out=None):
            self.before_gold = before_gold
            self.after_gold = after_gold
            self.fp = fp
            self.verify_fp = verify_fp
            self.update_out = update_out
            self.after_id = after_id
            self.before_count = before_count
            self.after_count = after_count
            self.columns_out = columns_out

        def run(self, side, tag, argv, env=None, timeout=120, stdin_text=None,
                capture=True):
            if tag == "assets.columns":
                return 0, (self.columns_out
                           or "Id\nChrName\nLoginID\nGold\nDeleted\n"), ""
            if tag == "assets.read":
                return 0, "%d\t6228\t%d\t%s\t0\n" % (self.before_count,
                                                          self.before_gold,
                                                          self.fp), ""
            if tag == "assets.update":
                return 0, self.update_out, ""
            if tag == "assets.verify":
                return 0, "%d\t%d\t%d\t%s\t0\n" % (self.after_count,
                                                        self.after_id,
                                                        self.after_gold,
                                                        self.verify_fp or self.fp), ""
            return 1, "", "unexpected tag %s" % tag

    prof = {"char_name": "winplay0", "account": "winplay0",
            "sides": {"csharp": {"db": {"characters_db": "mir2_test_db"}}}}
    sc = prof["sides"]["csharp"]
    decl_only = {"Gold": 10000, "note": ""}
    orig_pair = globals()["_check_db_pairing"]
    orig_root = globals()["placement_envir_root"]
    globals()["_check_db_pairing"] = lambda *a, **k: None
    globals()["placement_envir_root"] = lambda *a, **k: tmp
    try:
        ok = run_assets(_FakeRunner(), "csharp", prof, sc, decl_only, {})
        upd = (ok.get("sql") or {}).get("update") or ""
        out.append((ok["rc"] == 0 and ok["action"] == "updated"
                    and "UPDATE" in upd and "`Gold` = 10000" in upd
                    and ok["after"]["values"] == {"Gold": 10000},
                    "ASSET5 生产路径：假 runner 走通 run_assets（rc=0、UPDATE 只 SET "
                    "Gold=10000、读回相符）"))
        noop = run_assets(_FakeRunner(before_gold=10000), "csharp", prof, sc,
                          decl_only, {})
        out.append((noop["rc"] == 0 and noop["action"] == "noop"
                    and not (noop.get("sql") or {}).get("update"),
                    "ASSET5b 已是目标值 => noop（不打 UPDATE、rc=0）"))
        bad_fp = run_assets(_FakeRunner(verify_fp="b" * 32), "csharp", prof, sc,
                            decl_only, {})
        out.append((bad_fp["rc"] == 2 and "其它列" in (bad_fp["error"] or ""),
                    "ASSET6a 写后其余列指纹变化 => rc=2（连带写坏即红）"))
        bad_val = run_assets(_FakeRunner(after_gold=9999), "csharp", prof, sc,
                             decl_only, {})
        out.append((bad_val["rc"] == 2 and "读回与目标不符" in (bad_val["error"] or ""),
                    "ASSET6b 写后读回与目标不符 => rc=2（读回断言）"))
        bad_upd = run_assets(_FakeRunner(update_out="0\n"), "csharp", prof, sc,
                             decl_only, {})
        out.append((bad_upd["rc"] == 2 and "影响了 0 行" in (bad_upd["error"] or ""),
                    "ASSET6c UPDATE 影响 0 行 => rc=2（行被删/改）"))
        bad_char = run_assets(_FakeRunner(before_count=0), "csharp", prof, sc,
                              decl_only, {})
        out.append((bad_char["rc"] == 2 and "找不到角色" in (bad_char["error"] or ""),
                    "ASSET6d 角色不存在 => rc=2"))
        # 牙6e（负向）：声明 GamePoint 但实表缺列 => 拒跑（不是静默 0）
        miss = run_assets(_FakeRunner(columns_out="Id\nChrName\nLoginID\nGold\nDeleted\n"),
                          "csharp", prof, sc, {"GamePoint": 9, "note": ""}, {})
        out.append((miss["rc"] == 2 and "GamePoint" in (miss["error"] or "")
                    and "缺列" in (miss["error"] or ""),
                    "ASSET 牙6e 声明 GamePoint 但 characters 缺列 => rc=2（拒绝静默 0）"))

        # 牙③（正牙）：dry-run 预览行出现「列=目标值」且 rc=0
        class _DryRunner:
            dry_run = True

            def __init__(self):
                self.lines = []

            def log(self, text):
                self.lines.append(text)

            def run(self, *a, **k):
                return 0, "", ""

        dr = _DryRunner()
        dry = run_assets(dr, "csharp", prof, sc, decl_only, {})
        dry_logs = dr.lines
        out.append((dry["rc"] == 0 and dry["action"] == "dry-run"
                    and any("资产声明通道（dry-run 不连库）" in l and "Gold=10000" in l
                            for l in dry_logs),
                    "ASSET 牙3 dry-run 预览行出现声明列目标值（Gold=10000）且 rc=0"))
    finally:
        globals()["_check_db_pairing"] = orig_pair
        globals()["placement_envir_root"] = orig_root
    # 生产接线顺序：run_side 必须调用 run_assets，且排在 stack 之前（防旁路/挪位）
    import inspect
    side_src = inspect.getsource(run_side)
    out.append(("if assets:" in side_src
                and "run_assets(" in side_src
                and side_src.index("if assets:") < side_src.index("run_assets(")
                and side_src.index("run_assets(") < side_src.index('"stack"'),
                "ASSET7 run_side 生产接线：if assets: 走 run_assets，且在起栈之前"))
    return out

def _selftest_rng_ledger(tmp):
    """卡 openmir2-window-kit-p0-unblock：RNG 账本校验器 + 接线（--rng-log）。
    正例：头两行自证 + 5 列且 site 齐；负例：缺文件/空/缺自证/列数不足/site 空。"""
    out = []
    d = os.path.join(tmp, "rngledger")
    os.makedirs(d, exist_ok=True)

    def write(name, text):
        pth = os.path.join(d, name)
        with open(pth, "w", encoding="utf-8", newline="") as f:
            f.write(text)
        return pth

    good = write("good.tsv",
                 "1\tINJECT_OK\t42\t0\t\n"
                 "2\tSEED_PROBE\t8\t0\t\n"
                 "3\trand_range\t100\t7\tcrates/world/src/combat.rs:105\n"
                 "4\trand\t10\t3\tcrates/world/src/mongen.rs:88\n")

    ok, detail = check_rng_ledger(good)
    out.append((ok and "INJECT_OK" in detail and "SEED_PROBE" in detail,
                "RNG1 正例：头两行自证 + 5 列 site 齐 => ok（%s）" % detail))
    renv = rng_env(d)
    out.append((renv.get("MIR2_RNG_SITE") == "1"
                and renv.get("MIR2_RNG_LOG", "").endswith("rng-stream.tsv"),
                "RNG1b rng_env 同时给 MIR2_RNG_LOG 与 MIR2_RNG_SITE=1（C# 钩子写 5 列）"))
    ok2, _ = check_rng_ledger(os.path.join(d, "nope.tsv"))
    out.append((not ok2, "RNG 牙1 缺文件 => 红"))
    ok3, _ = check_rng_ledger(write("empty.tsv", ""))
    out.append((not ok3, "RNG 牙2 空文件 => 红"))
    noinject = write("noinject.tsv",
                     "1\tSEED_PROBE\t8\t0\t\n"
                     "2\trand\t10\t3\ta.rs:1\n")
    ok4, e4 = check_rng_ledger(noinject)
    out.append((not ok4 and "INJECT_OK" in e4, "RNG 牙3 缺 INJECT_OK => 红且点名"))
    short = write("short.tsv",
                  "1\tINJECT_OK\t42\t0\t\n"
                  "2\tSEED_PROBE\t8\t0\t\n"
                  "3\trand\t10\t3\n")
    ok5, e5 = check_rng_ledger(short)
    out.append((not ok5 and "列数" in e5, "RNG 牙4 数据行列数不足 => 红且点名"))
    # 牙6（review P-1 的反向）：C# 钩子不设 MIR2_RNG_SITE 时写 4 列（seq/kind/arg/result）
    # ⇒ 校验必须红——证明注入 SITE 是判据的一部分，不是装饰。
    csharp4 = write("csharp-nosite.tsv",
                    "1'+T+'INJECT_OK'+T+'42'+T+'0'+Nn+'"
                    "2'+T+'SEED_PROBE'+T+'8'+T+'0'+Nn+'"
                    "3'+T+'rand'+T+'10'+T+'3'+Nn+'")
    ok7, e7 = check_rng_ledger(csharp4)
    out.append((not ok7 and ("site" in e7.lower() or "列数" in e7),
                "RNG 牙6 C# 钩子未设 MIR2_RNG_SITE 的 4 列账本 => 红（拒绝）"))
    nosi = write("nosite.tsv",
                 "1\tINJECT_OK\t42\t0\t\n"
                 "2\tSEED_PROBE\t8\t0\t\n"
                 "3\trand\t10\t3\t\n")
    ok6, e6 = check_rng_ledger(nosi)
    out.append((not ok6 and "site" in e6, "RNG 牙5 site 为空 => 红且点名"))
    import inspect
    src = inspect.getsource(run_side)
    out.append(("MIR2_RNG_LOG" in src and "MIR2_RNG_SITE" in src
                and "check_rng_ledger(" in src
                and src.index("MIR2_RNG_LOG") < src.index(chr(34)+"stack"+chr(34)),
                "RNG2 接线：run_side 在起栈前注入 MIR2_RNG_LOG+MIR2_RNG_SITE，成功前校验账本"))
    mod = inspect.getsource(sys.modules[__name__])
    out.append(("--rng-log" in mod and "rng_log" in mod,
                "RNG3 CLI --rng-log 与 cfg/ctx 透传在位"))
    return out


def _selftest_fixtures_files(tmp, stub=None, log=None, base_cfg=None):
    """卡 `openmir2-window-fixture-castle`：配置覆盖 + 城堡前置（离线，不碰真实部署）。

    与 `_selftest_fixtures`（怪物夹具）共用同一 `profile.fixtures` schema 与 `targets[]`
    证据；本函数覆盖文件侧、F-1 对称性、F1 部分回滚、G-1 影子根（文件）。
    """
    out = []
    root = os.path.join(tmp, "fixture-deploy")
    os.makedirs(os.path.join(root, "Castle", "0"), exist_ok=True)
    os.makedirs(os.path.join(root, "Envir"), exist_ok=True)
    conf = os.path.join(root, "Server.conf")
    conf_body = (b"[Setup]\r\nCastleTaxRate=5\r\nCastleGetAllNpcTax=0\r\n"
                 b"ConsoleShowUserCountTime=600000\r\n")
    with open(conf, "wb") as f:
        f.write(conf_body)
    list_path = os.path.join(root, "Castle", "List.txt")
    with open(list_path, "wb") as f:
        f.write(b"0\r\n")
    sabuk = os.path.join(root, "Castle", "0", "SabukW.txt")
    sabuk_body = ("[Setup]\r\nCastleName=沙巴克\r\nOwnGuild=\r\nTotalGold=0\r\n"
                  "TodayIncome=0\r\n\r\n[Defense]\r\nCastleMap=3\r\n").encode("gbk")
    with open(sabuk, "wb") as f:
        f.write(sabuk_body)
    with open(os.path.join(root, "Envir", "MapInfo.txt"), "wb") as f:
        f.write("[0 主城]\n[0153 药品店]\n".encode("gbk"))
    with open(os.path.join(root, "Envir", "MonGen.txt"), "wb") as f:
        f.write("3\t294\t341\t神鹰\t10\t1\t360\n".encode("gbk"))
    sc = {"envir_root": root}
    side_dir = os.path.join(tmp, "fixture-side")
    os.makedirs(side_dir, exist_ok=True)

    def readb(path):
        with open(path, "rb") as f:
            return f.read()

    raw_decl = {"config_overrides": {"Server.conf": {"CastleGetAllNpcTax": 1}},
                "castle_list": ["0"],
                "castle_files": {"0": {"OwnGuild": "沙巴克行会", "TotalGold": 4}},
                "note": "W-TAX-2 窗口副本"}
    decl = _validate_fixture_decl(raw_decl, "selftest.fixtures")
    dry_runner = Runner(tmp, None, dry_run=True)
    pre_sha = hashlib.sha256(conf_body).hexdigest()

    # ① dry-run：只打印 diff，不写文件、不建备份
    dres = run_fixtures(dry_runner, "rust", {}, sc, decl, {}, side_dir)
    out.append((dres.get("rc") == 0 and dres.get("action") == "dry-run",
                "FX1 dry-run 合法声明 => rc=0（action=dry-run）"))
    out.append((readb(conf) == conf_body
                and not os.path.exists(os.path.join(side_dir, "fixtures")),
                "FX2 dry-run 不写文件、不建备份目录"))

    # ② 真写：只改目标行；写前/写后 sha256 可复核
    runner = Runner(tmp, None, dry_run=False)
    res = run_fixtures(runner, "rust", {}, sc, decl, {}, side_dir)
    out.append((res.get("rc") == 0 and res.get("action") == "applied",
                "FX4 apply rc=0（action=applied）"))
    expect_conf = conf_body.replace(b"CastleGetAllNpcTax=0", b"CastleGetAllNpcTax=1")
    out.append((readb(conf) == expect_conf,
                "FX5 Server.conf 只改目标值（其余字节逐字节不变，含 CRLF）"))
    rec0 = res["files"][0]
    out.append((rec0["before_sha256"] == pre_sha
                and rec0["after_sha256"] == hashlib.sha256(expect_conf).hexdigest()
                and [t for t in res["targets"] if t.get("rel") == "Server.conf"],
                "FX6 写前/写后 sha256 与实测一致（且进 targets[]）"))
    new_sabuk = readb(sabuk)
    out.append((b"OwnGuild=" + "沙巴克行会".encode("gbk") + b"\r\n" in new_sabuk
                and b"TotalGold=4\r\n" in new_sabuk
                and new_sabuk.endswith(b"CastleMap=3\r\n"),
                "FX7 SabukW OwnGuild 按 GBK 写入、TotalGold=4、[Defense] 段未动"))
    out.append((readb(list_path) == b"0\r\n"
                and any(r["rel"] == "Castle/List.txt" and r["changed"] is False
                        for r in res["files"]),
                "FX8 List.txt 同城集合 => 内容不变但仍留 sha 记录（changed=False）"))

    # ③ 期间被第三方改写（模拟服务端 10 分钟存档）+ restore：按备份还原到写前字节
    with open(conf, "ab") as f:
        f.write(b"CastleTaxRate=9\r\n")
    err = restore_fixtures(res)
    out.append((err is None, "FX9 restore 成功（%s）" % (err,)))
    out.append((readb(conf) == conf_body
                and hashlib.sha256(readb(conf)).hexdigest() == pre_sha,
                "FX10 Server.conf 按 sha256 还原到写前字节（即使期间被外部改过）"))
    out.append((readb(sabuk) == sabuk_body, "FX11 SabukW 还原到写前字节"))
    out.append((all(r.get("backup") is None for r in res["files"]),
                "FX12 还原后备份文件已清理（backup=None）"))

    # ④ no-op：值已等于目标
    res2 = run_fixtures(runner, "rust", {}, sc, _validate_fixture_decl(
        {"config_overrides": {"Server.conf": {"CastleGetAllNpcTax": 0}}},
        "selftest.fixtures"), {}, side_dir)
    out.append((res2.get("rc") == 0 and res2["files"][0]["changed"] is False
                and res2["files"][0]["before_sha256"]
                == res2["files"][0]["after_sha256"],
                "FX13 值已等于目标 => changed=False（sha 不变）"))
    restore_fixtures(res2)

    # ⑤ 声明校验：未知文件/键、越界、坏城名、重复、空、只有 note
    def rejects(body, needle):
        try:
            _validate_fixture_decl(body, "profile.fixtures")
            return False
        except ConfigError as e:
            return needle in str(e)

    out.append((rejects({"config_overrides": {"Server.conf": {"BogusKey": 1}}},
                        "未白名单的配置键"), "FX14 未知配置键 => ConfigError"))
    out.append((rejects({"config_overrides": {"Other.conf": {"CastleTaxRate": 5}}},
                        "未白名单的配置文件"), "FX15 未知配置文件 => ConfigError"))
    out.append((rejects({"config_overrides": {"Server.conf": {"CastleGetAllNpcTax": 2}}},
                        "越界"), "FX16 越界值（CastleGetAllNpcTax=2）=> ConfigError"))
    out.append((rejects({"castle_list": ["0", "0"]}, "重复"), "FX17 重复城 => ConfigError"))
    out.append((rejects({"castle_list": []}, "非空"), "FX18 空城列表 => ConfigError"))
    out.append((rejects({"castle_list": ["00"]}, "规范十进制"),
                "FX19 前导零城名 => ConfigError"))
    out.append((rejects({"castle_files": {"1": {"TotalGold": 1}}, "castle_list": ["0"]},
                        "不在同一 fixtures"),
                "FX20 castle_files 城不在 castle_list => ConfigError"))
    out.append((rejects({"castle_files": {"0": {"OwnGuild": "甲" * 40}}}, "> 32"),
                "FX21 OwnGuild 超 32 字节 => ConfigError"))
    out.append((rejects({"castle_files": {"0": {"OwnGuild": "a\nb"}}}, "换行"),
                "FX22 OwnGuild 含换行 => ConfigError"))
    out.append((rejects({"note": "只有 note"}, "至少要给"),
                "FX23 只有 note => ConfigError"))
    out.append((rejects({"fixtures_typo": {}}, "未知字段"),
                "FX24 顶层未知字段 => ConfigError"))

    # ⑥ 文件缺失 / 键不唯一 / 生产根守卫
    rmiss = run_fixtures(runner, "rust", {}, sc, _validate_fixture_decl(
        {"config_overrides": {"Server1.conf": {"CastleTaxRate": 5}}},
        "selftest.fixtures"), {}, side_dir)
    out.append((rmiss.get("rc") == 2 and "不存在" in (rmiss.get("error") or ""),
                "FX25 目标文件缺失 => rc=2（不创建/不猜）"))
    with open(conf, "wb") as f:
        f.write(conf_body + b"CastleGetAllNpcTax=0\r\n")
    rdup = run_fixtures(runner, "rust", {}, sc, decl, {}, side_dir)
    out.append((rdup.get("rc") == 2 and "出现 2 次" in (rdup.get("error") or ""),
                "FX26 键出现 2 次 => rc=2（不猜）"))
    with open(conf, "wb") as f:
        f.write(conf_body)
    rprod = run_fixtures(runner, "rust", {}, {"envir_root": FIXTURE_PROD_ROOT},
                         decl, {}, side_dir)
    out.append((rprod.get("rc") == 2 and "生产内容根" in (rprod.get("error") or ""),
                "FX27 envir_root 指向 E:/MirServer => rc=2（内容冻结）"))

    # ⑦ sides.<side>.fixtures 整体覆盖顶层
    prof_both = {"fixtures": {"config_overrides":
                              {"Server.conf": {"CastleGetAllNpcTax": 1}}},
                 "sides": {"rust": {"fixtures": {"config_overrides":
                                                 {"Server.conf":
                                                  {"CastleGetAllNpcTax": 0}}}}}}
    out.append((profile_fixtures(prof_both, "rust")["config_overrides"]["Server.conf"]
                ["CastleGetAllNpcTax"] == 0
                and profile_fixtures(prof_both, "csharp")["config_overrides"]
                ["Server.conf"]["CastleGetAllNpcTax"] == 1,
                "FX28 sides.<side>.fixtures 整体覆盖顶层（对 config_overrides 成立；"
                "castle_list 受加载期对称性约束，见 FX36）"))

    # ⑫ castle_list 两侧对称（review F-1）：门在写前、夹具在写后 ⇒ 加载期必须拦
    def sym(decls):
        try:
            _check_fixture_symmetry(decls)
            return None
        except ConfigError as e:
            return str(e)

    err36 = sym({"csharp": {"castle_list": ["0"]},
                 "rust": {"castle_list": ["0", "1"]}})
    out.append((err36 is not None and "不一致" in err36,
                "FX36 两侧 castle_list 不同 => ConfigError（加载期拦，不靠门）"))
    out.append((sym({"csharp": {"castle_list": ["0"]},
                     "rust": {"castle_list": ["0"]}}) is None
                and sym({"csharp": None, "rust": None}) is None,
                "FX37 两侧相同 / 两侧都不声明 => 通过"))
    err38 = sym({"csharp": {"castle_list": ["0"]}, "rust": None})
    out.append((err38 is not None and "不一致" in err38,
                "FX38 一侧声明/一侧不声明 => ConfigError（避免只写一侧造成不对称）"))
    bad_prof = {"fixtures": {"castle_list": ["0"]},
                "sides": {"csharp": {},
                          "rust": {"fixtures": {"castle_list": ["0", "1"]}}}}
    err40 = sym({sd: profile_fixtures(bad_prof, sd) for sd in SIDES})
    out.append((err40 is not None and "不一致" in err40,
                "FX40 复现 reviewer 的绕过声明经 profile_fixtures 后仍被拒"))
    err48 = sym({sd: profile_fixtures(
        {"fixtures": {"config_overrides": {"Server.conf": {"CastleTaxRate": 5}}},
         "sides": {"rust": {"fixtures": {"config_overrides":
                                         {"Server.conf": {"CastleTaxRate": 9}}}}}},
        sd) for sd in SIDES})
    out.append((err48 is not None and "不一致" in err48,
                "FX48 config_overrides 两侧不同 => ConfigError（对拍无效，不沉默）"))
    err49 = sym({sd: profile_fixtures(
        {"fixtures": {"castle_files": {"0": {"TotalGold": 0}}},
         "sides": {"rust": {"fixtures": {"castle_files": {"0": {"TotalGold": 7}}}}}},
        sd) for sd in SIDES})
    out.append((err49 is not None and "不一致" in err49,
                "FX49 castle_files 两侧不同 => ConfigError（对拍无效，不沉默）"))

    # ⑬ review F1：逐文件写时第 2 个失败 ⇒ 第 1 个必须已就地回滚（不留污染）
    conf_before = readb(conf)
    sabuk_before = readb(sabuk)
    saved_w = globals()["_fixture_atomic_write"]
    state = {"hit": 0}

    def _bad_write(path, data):
        if path.endswith("SabukW.txt") and state["hit"] == 0:
            state["hit"] += 1
            with open(path, "wb") as f:
                f.write(b"CORRUPT")
            return
        saved_w(path, data)

    globals()["_fixture_atomic_write"] = _bad_write
    try:
        res_bad = run_fixtures(runner, "rust", {}, sc, decl, {}, side_dir)
    finally:
        globals()["_fixture_atomic_write"] = saved_w
    leftover = []
    bdir = os.path.join(side_dir, "fixtures", "backup")
    if os.path.isdir(bdir):
        leftover = sorted(os.listdir(bdir))
    rolled = [r for r in (res_bad.get("files") or [])
              if r.get("action") == "rolled-back"]
    out.append((res_bad.get("rc") == 2
                and "写后读回与目标不符" in (res_bad.get("error") or "")
                and readb(conf) == conf_before and readb(sabuk) == sabuk_before
                and not leftover and rolled
                and all(r.get("restore_sha256") == r.get("before_sha256")
                        for r in rolled),
                "FX42 第 2 个文件写坏 ⇒ rc=2 且**已写的第 1 个文件就地回滚**"
                "（conf/sabuk 回原字节、无备份残留）"))

    # ⑭ G-1 影子根（文件侧）：基线 + M2GameSvr-hook 两个根都写、都还原、都进 targets[]
    sp = os.path.join(tmp, "shadow-deploy")
    base = os.path.join(sp, "M2GameSvr")
    shadow = os.path.join(sp, "M2GameSvr-hook")
    shadow_before = {}
    crlf = bytes([13, 10])
    for r in (base, shadow):
        os.makedirs(os.path.join(r, "Castle", "0"), exist_ok=True)
        os.makedirs(os.path.join(r, "Envir"), exist_ok=True)
        with open(os.path.join(r, "Server.conf"), "wb") as f:
            f.write(conf_body)
        with open(os.path.join(r, "Castle", "List.txt"), "wb") as f:
            f.write(b"0" + crlf)
        with open(os.path.join(r, "Castle", "0", "SabukW.txt"), "wb") as f:
            f.write(sabuk_body)
        with open(os.path.join(r, "Envir", "MapInfo.txt"), "wb") as f:
            f.write("[0 主城]".encode("gbk") + bytes([10]))
        with open(os.path.join(r, "Envir", "MonGen.txt"), "wb") as f:
            f.write(("3" + chr(9) + "294" + chr(9) + "341" + chr(9) + "神鹰"
                     + chr(9) + "10" + chr(9) + "1" + chr(9) + "360").encode("gbk")
                    + bytes([10]))
        shadow_before[r] = (readb(os.path.join(r, "Server.conf")),
                            readb(os.path.join(r, "Castle", "0", "SabukW.txt")))
    sc2 = {"envir_root": base}
    decl2 = _validate_fixture_decl(
        {"config_overrides": {"Server.conf": {"CastleGetAllNpcTax": 1}},
         "castle_files": {"0": {"TotalGold": 4}}}, "selftest.fixtures")
    res_s = run_fixtures(runner, "csharp", {}, sc2, decl2, {}, side_dir)
    file_tgts = [t for t in res_s.get("targets") or [] if t.get("rel")]
    both_conf_ok = all(readb(os.path.join(r, "Server.conf")) == expect_conf
                       for r in (base, shadow))
    both_sabuk_ok = all(b"TotalGold=4" in readb(os.path.join(r, "Castle", "0",
                                                             "SabukW.txt"))
                        for r in (base, shadow))
    out.append((res_s.get("rc") == 0
                and res_s.get("roots") == [base, shadow]
                and {t.get("target") for t in file_tgts} == {"baseline", "shadow"}
                and len(file_tgts) == 4 and both_conf_ok and both_sabuk_ok,
                "FX43 影子根：M2GameSvr 与 M2GameSvr-hook 写同一内容"
                "（targets[] 含 baseline/shadow 各 2 个文件）"))
    err_s = restore_fixtures(res_s)
    out.append((err_s is None
                and all(readb(os.path.join(r, "Server.conf")) == shadow_before[r][0]
                        and readb(os.path.join(r, "Castle", "0", "SabukW.txt"))
                        == shadow_before[r][1] for r in (base, shadow)),
                "FX44 影子根还原：两个根的 Server.conf 与 SabukW 都回到写前字节"))
    res_d = run_fixtures(dry_runner, "csharp", {}, sc2, decl2, {}, side_dir)
    out.append((res_d.get("action") == "dry-run"
                and res_d.get("roots") == [base, shadow]
                and all(readb(os.path.join(r, "Server.conf"))
                        == shadow_before[r][0] for r in (base, shadow)),
                "FX45 影子根 dry-run：预览两个根、两个根都不落盘"))

    # ⑮ 合并声明（monsters + 文件）：action=prepended+applied，两类各自还原
    conf_before2 = readb(conf)
    mongen_path = os.path.join(root, "Envir", "MonGen.txt")
    mongen_before2 = readb(mongen_path)
    decl3 = _validate_fixture_decl(
        {"monsters": [{"map": "0153", "x": 3, "y": 4, "name": "鸡"}],
         "config_overrides": {"Server.conf": {"CastleGetAllNpcTax": 1}}},
        "selftest.fixtures")
    res3 = run_fixtures(runner, "rust", {}, sc, decl3, {}, side_dir)
    want_m = ("0153" + chr(9) + "3" + chr(9) + "4" + chr(9) + "鸡" + chr(9)
              + "0" + chr(9) + "1" + chr(9) + "360").encode("gbk")
    out.append((res3.get("rc") == 0 and res3.get("action") == "prepended+applied"
                and want_m in readb(mongen_path) and readb(conf) == expect_conf
                and any(not t.get("rel") for t in res3.get("targets") or [])
                and any(t.get("rel") == "Server.conf"
                        for t in res3.get("targets") or []),
                "FX46 合并声明：MonGen 前插 + Server.conf 覆盖，action=prepended+applied，"
                "targets[] 同时含怪物与文件"))
    err3 = restore_fixtures(res3)
    out.append((err3 is None and readb(mongen_path) == mongen_before2
                and readb(conf) == conf_before2,
                "FX47 合并声明的还原：MonGen 与配置文件都回到写前字节"))

    # ⑨ 牙：白名单摘掉 CastleGetAllNpcTax => 同一份合法声明必须被拒
    saved = dict(FIXTURE_CONFIG_KEYS)
    try:
        FIXTURE_CONFIG_KEYS.pop("CastleGetAllNpcTax")
        tooth_red = False
        try:
            _validate_fixture_decl(raw_decl, "profile.fixtures")
        except ConfigError as e:
            tooth_red = "未白名单的配置键" in str(e)
    finally:
        FIXTURE_CONFIG_KEYS.clear()
        FIXTURE_CONFIG_KEYS.update(saved)
    out.append((tooth_red,
                "FX 牙 白名单摘掉 CastleGetAllNpcTax => 同一声明被拒（白名单承重）"))

    # ⑩ 生产接线（源码级）
    import inspect
    src_side = inspect.getsource(run_side)
    out.append(("run_fixtures(" in src_side
                and src_side.index("run_fixtures(") < src_side.index("# 2) 起栈前端口检查"),
                "FX29 run_side 生产接线：夹具在起栈之前应用"))
    out.append(("restore_fixtures(res[\"fixtures\"])" in src_side
                and src_side.index("restore_fixtures(res[\"fixtures\"])")
                > src_side.index("finally:"),
                "FX30 run_side finally 在停栈之后还原夹具"))
    out.append(("profile_fixtures(prof, side)" in inspect.getsource(load_profile),
                "FX31 load_profile 加载期校验 profile_fixtures"))
    out.append(("_check_fixture_symmetry(fixtures_by_side)"
                in inspect.getsource(load_profile),
                "FX41 load_profile 实际调用 castle_list 对称性检查（接线，不只是函数存在）"))

    # ⑪ execute 级（dry-run）
    if base_cfg is not None:
        prof = _selftest_profile(tmp, stub, log,
                                 envir_roots={"csharp": root, "rust": root})
        prof["fixtures"] = {"config_overrides":
                            {"Server.conf": {"CastleGetAllNpcTax": 1}}}
        summary = execute(base_cfg(os.path.join(tmp, "fx-exec"), prof, dry_run=True))
        fside = ((summary.get("fixtures") or {}).get("sides") or {}).get("rust") or {}
        out.append((summary.get("exit_code") == 0
                    and (summary.get("fixtures") or {}).get("enabled") is True
                    and fside.get("rc") == 0 and fside.get("action") == "dry-run",
                    "FX32 execute(dry-run) 走通夹具预览：summary.fixtures 有证据"))
        out.append((readb(conf) == conf_body,
                    "FX33 execute(dry-run) 之后 Server.conf 未被改写"))
    return out


def _selftest_fixtures(tmp):
    """卡 openmir2-window-fixture-corpse：声明式怪物夹具（解析/负例/写+还原/接线）。"""
    import json as _json
    out = []
    d = os.path.join(tmp, "fixturesel")
    root = os.path.join(d, "root")
    os.makedirs(os.path.join(root, "Envir"), exist_ok=True)
    with open(os.path.join(root, "Envir", "MapInfo.txt"), "w",
              encoding="gbk", newline="\n") as f:
        f.write("[0 主城]\n[3 盟重省]\n")
    mongen = os.path.join(root, "Envir", "MonGen.txt")
    with open(mongen, "wb") as f:
        f.write("3\t294\t341\t神鹰\t10\t1\t360\n".encode("gbk"))

    prof = {"sides": {"csharp": {"envir_root": root,
                                 "fixtures": {"monsters": [
                                     {"map": "0", "x": 330, "y": 331, "name": "鸡"}]}}}}
    decl = profile_fixtures(prof, "csharp")
    # 卡 openmir2-window-fixture-castle：schema 与文件夹具合并后，归一化结果同时含文件三件套
    # （未声明时为空集）——断言收敛到 monsters 本体 + 文件侧为空。
    out.append((decl["monsters"] == [{"map": "0", "x": 330, "y": 331,
                                     "name": "鸡", "count": 1}]
                and decl["config_overrides"] == {} and decl["castle_list"] is None
                and decl["castle_files"] == {},
                "FIX1 合法声明（map/x/y/name，count 缺省 1）=> 解析透传（文件侧为空）"))
    out.append((profile_fixtures({"sides": {"csharp": {}}}, "csharp") is None,
                "FIX1b 未声明 fixtures => None（不使用夹具的场景行为不变）"))

    bad_cases = [
        ({"fixtures": {"foo": []}}, "未知字段"),
        ({"fixtures": {"monsters": []}}, "非空数组"),
        ({"fixtures": {"monsters": [{"map": "0", "x": 1, "y": 2}]}}, "缺字段 name"),
        ({"fixtures": {"monsters": [{"map": "0", "x": -1, "y": 2, "name": "鸡"}]}}, "0..65535"),
        ({"fixtures": {"monsters": [{"map": "0", "x": 1, "y": 2, "name": "鸡",
                                     "extra": 1}]}}, "未知字段"),
        ({"fixtures": {"monsters": [{"map": "0", "x": 1, "y": 2, "name": "鸡",
                                     "count": 0}]}}, "1..100"),
        ({"fixtures": {"monsters": [{"map": "0", "x": 1, "y": 2,
                                     "name": "\U0001F600"}]}}, "GBK"),
    ]
    bad_ok = True
    for side_doc, word in bad_cases:
        sc = {"envir_root": root}
        sc.update(side_doc)
        try:
            profile_fixtures({"sides": {"csharp": sc}}, "csharp")
            bad_ok = False
        except ConfigError as ex:
            if word not in str(ex):
                bad_ok = False
    out.append((bad_ok, "FIX 牙1 未知字段/空数组/缺字段/越界/count 0/非 GBK => ConfigError 点名"))

    class _R:
        dry_run = False

        def __init__(self):
            self.lines = []

        def log(self, t):
            self.lines.append(t)

    class _RD:
        dry_run = True

        def __init__(self):
            self.lines = []

        def log(self, t):
            self.lines.append(t)

    r = _R()
    side_dir = os.path.join(d, "side")
    res = run_fixtures(r, "csharp", prof, prof["sides"]["csharp"], decl, {}, side_dir)
    raw_after = io.open(mongen, "rb").read()
    out.append((res.get("rc") == 0 and res.get("action") == "prepended"
                and "0\t330\t331\t鸡\t0\t1\t360".encode("gbk") in raw_after
                and os.path.exists(res.get("backup") or ""),
                "FIX2 写 Envir/MonGen.txt：追加单点刷怪行、备份存在、rc=0"))
    line = res["prepend"][0]
    cols = line.split("\t")
    zen_min = int(cols[6]) if len(cols) >= 7 and cols[6].lstrip("-").isdigit() else -1
    zen_ms = zen_min * 60 * 1000
    out.append((len(cols) >= 7 and zen_min > 0 and zen_ms > 0,
                "FIX2b 夹具行第 7 列 ZenTime>0（%s 分钟 => %d ms）——按两端解析器口径"
                "（zen_time<=0 即丢弃且无日志）不会被静默丢行" % (zen_min, zen_ms)))

    # FIX2c（review G-1）：hook 影子存在时，夹具必须**同时**写基线与影子（sha 一致）。
    shadow_root = os.path.join(d, "M2GameSvr-hook")
    os.makedirs(os.path.join(shadow_root, "Envir"), exist_ok=True)
    shadow_mongen = os.path.join(shadow_root, "Envir", "MonGen.txt")
    with open(mongen, "rb") as f:
        _raw = f.read()
    with open(shadow_mongen, "wb") as f:
        f.write(_raw)
    prof_sh = {"sides": {"csharp": {"envir_root": root,
                                    "fixtures": {"monsters": [
                                        {"map": "0", "x": 330, "y": 331,
                                         "name": "鸡", "count": 1}]}}}}
    side_dir_sh = os.path.join(d, "side-shadow")
    res_sh = run_fixtures(_R(), "csharp", prof_sh, prof_sh["sides"]["csharp"],
                          profile_fixtures(prof_sh, "csharp"), {}, side_dir_sh)
    tgts = res_sh.get("targets") or []
    with open(mongen, "rb") as f:
        b1 = f.read()
    with open(shadow_mongen, "rb") as f:
        b2 = f.read()
    line_bytes = "0\t330\t331\t鸡\t0\t1\t360".encode("gbk")
    out.append((res_sh.get("rc") == 0 and len(tgts) == 2
                and {t.get("target") for t in tgts} == {"baseline", "shadow"}
                and line_bytes in b1 and line_bytes in b2 and b1 == b2,
                "FIX2c G-1：影子存在时同一夹具同时写基线与影子（两侧内容逐字节相同）"))
    rerr = restore_fixtures(res_sh)
    with open(mongen, "rb") as f:
        r1 = f.read()
    with open(shadow_mongen, "rb") as f:
        r2 = f.read()
    out.append((rerr is None and r1 == _raw and r2 == _raw,
                "FIX3c G-1：基线与影子都按 sha256 还原（重读磁盘）"))
    err = restore_fixtures(res)
    raw_restored = io.open(mongen, "rb").read()
    out.append((err is None and hashlib.sha256(raw_restored).hexdigest()
                == res.get("before_sha256"),
                "FIX3 停栈后还原：sha256 与写前一致"))

    rd = _RD()
    res_dry = run_fixtures(rd, "csharp", prof, prof["sides"]["csharp"], decl, {}, side_dir)
    out.append((res_dry.get("rc") == 0 and res_dry.get("action") == "dry-run"
                and any("dry-run 不写 Envir" in l for l in rd.lines),
                "FIX 牙2 dry-run：只打印预览、不写文件"))

    bad_prof = {"sides": {"csharp": {"envir_root": root,
                                     "fixtures": {"monsters": [
                                         {"map": "NOPE", "x": 1, "y": 2, "name": "鸡"}]}}}}
    res_bad = run_fixtures(_R(), "csharp", bad_prof, bad_prof["sides"]["csharp"],
                           profile_fixtures(bad_prof, "csharp"), {}, side_dir)
    out.append((res_bad.get("rc") == 2 and "MapInfo" in (res_bad.get("error") or ""),
                "FIX 牙3 地图不在本侧 MapInfo => rc=2 点名（fail-closed）"))

    import inspect
    src = inspect.getsource(run_side)
    out.append(("profile_fixtures(" in src and "run_fixtures(" in src
                and "restore_fixtures(" in src
                and src.index("run_fixtures(") < src.index('"stack"'),
                "FIX4 接线：run_side 起栈前应用夹具、finally 还原"))
    return out


def _selftest_level_fixture(tmp):
    """卡 openmir2-window-fixture-level-items：level 声明（解析/负例/对称/dry-run/写读回/还原）。"""
    out = []
    good = {"level": 43, "note": "t118 前置 checklevel 43"}
    d = _validate_fixture_decl(dict(good), "selftest.fixtures")
    out.append((d.get("level") == 43,
                "LEVEL1 声明 level=43 => 解析透传（note 不参与）"))
    ok_edges = all(_validate_fixture_decl({"level": v}, "x").get("level") == v
                   for v in (1, 255))
    out.append((ok_edges, "LEVEL1b 边界 1 与 255 放行"))

    bad_ok = True
    for v in (0, 256, -1, True, "43", 43.5, None):
        try:
            _validate_fixture_decl({"level": v}, "x")
            bad_ok = False
        except ConfigError as ex:
            if "1..255" not in str(ex):
                bad_ok = False
    out.append((bad_ok, "LEVEL 牙1 level 0/256/负数/bool/字符串/浮点/null => ConfigError 点名 1..255"))

    # 未知键仍 fail-closed（level 加入后不得放宽）
    try:
        _validate_fixture_decl({"levels": 43}, "x")
        unknown_ok = False
    except ConfigError as ex:
        unknown_ok = "levels" in str(ex) and "未知字段" in str(ex)
    out.append((unknown_ok, "LEVEL 牙2 未知键 levels => ConfigError 点名"))

    # 两侧 level 必须一致（文件三件套之外，DB 侧也纳入 symmetry key）
    try:
        _check_fixture_symmetry({"csharp": {"level": 43}, "rust": {"level": 44}})
        sym_ok = False
    except ConfigError as ex:
        sym_ok = "不一致" in str(ex) and "level=44" in str(ex)
    out.append((sym_ok, "LEVEL 牙3 两侧 level 不同 => ConfigError（对拍无效）"))

    class _Dry:
        dry_run = True

        def __init__(self):
            self.lines = []
            self.calls = []

        def log(self, t):
            self.lines.append(t)

        def run(self, *a, **k):
            self.calls.append(a)
            return 0, "", ""

    class _R:
        dry_run = False

        def __init__(self, read_level=1, verify_level=43, restore_level=1,
                     restore_ok=True):
            self.read_level = read_level
            self.verify_level = verify_level
            self.restore_level = restore_level
            self.restore_ok = restore_ok
            self.tags = []

        def run(self, side, tag, argv, env=None, timeout=120, stdin_text=None,
                capture=True):
            self.tags.append(tag)
            if tag == "level.columns":
                return 0, "Id\nChrName\nLoginID\nLevel\nDeleted\n", ""
            if tag in ("level.read",):
                return 0, "1\t6228\t%d\t%s\t0\n" % (self.read_level, "a" * 32), ""
            if tag == "level.update":
                return 0, "1\n", ""
            if tag == "level.verify":
                return 0, "1\t6228\t%d\t%s\t0\n" % (self.verify_level, "a" * 32), ""
            if tag == "level.restore":
                return 0, "1\n", ""
            if tag == "level.restore_verify":
                return 0, "1\t6228\t%d\t%s\t0\n" % (
                    self.restore_level if self.restore_ok else 99, "a" * 32), ""
            return 1, "", "unexpected tag %s" % tag

    prof = {"char_name": "winplay0", "account": "winplay0",
            "sides": {"csharp": {"db": {"characters_db": "mir2_test_db"}}}}
    sc = prof["sides"]["csharp"]
    orig = (globals()["_check_db_pairing"], globals()["placement_envir_root"],
            globals()["placement_characters_db"], globals()["placement_db_client"])
    globals()["_check_db_pairing"] = lambda *a, **k: None
    globals()["placement_envir_root"] = lambda *a, **k: tmp
    globals()["placement_characters_db"] = lambda side, sc_: "mir2_test_db"
    globals()["placement_db_client"] = lambda prof_, side_: {
        "mysql": [DEFAULT_MYSQL], "host": "127.0.0.1", "port": 3306,
        "user": "root", "password": ""}
    try:
        dr = _Dry()
        dres = run_level_fixture(dr, "csharp", prof, sc, 43, {}, tmp)
        out.append((dres.get("rc") == 0 and dres.get("action") == "dry-run"
                    and any("dry-run 不连库" in l and "Level=43" in l for l in dr.lines)
                    and not dr.calls,
                    "LEVEL2 dry-run：打印 UPDATE 预览且不执行任何 mysql"))

        r = _R()
        res = run_level_fixture(r, "csharp", prof, sc, 43, {}, tmp)
        restored = restore_level_fixture(r, res, {})
        out.append((res.get("rc") == 0 and res.get("action") == "updated"
                    and res.get("before_level") == 1 and res.get("after_level") == 43
                    and restored is None and "level.restore" in r.tags
                    and "level.restore_verify" in r.tags,
                    "LEVEL3 生产路径：写前 1 -> 写 43 -> 读回 43；停栈后还原并重读为 1"))

        # LEVEL 牙6（review Minor）：凭据不得进证据 dict / level.json
        globals()["placement_db_client"] = lambda prof_, side_: {
            "mysql": [DEFAULT_MYSQL], "host": "127.0.0.1", "port": 3306,
            "user": "root", "password": "SECRETPW"}
        r_sec = _R()
        res_sec = run_level_fixture(r_sec, "csharp", prof, sc, 43, {}, tmp)
        ev = level_evidence(res_sec)
        blob = json.dumps(ev, ensure_ascii=False)
        raw_blob = json.dumps(res_sec, ensure_ascii=False, default=str)
        out.append(("_session" in res_sec and "_session" not in ev
                    and "SECRETPW" not in blob and "--password" not in blob
                    and "SECRETPW" in raw_blob,
                    "LEVEL 牙6 凭据只留在内存 _session：level_evidence/level.json 不含 "
                    "--password/SECRETPW（原始 res 里确有此值，证明牙有效）"))

        r_noop = _R(read_level=43)
        noop = run_level_fixture(r_noop, "csharp", prof, sc, 43, {}, tmp)
        out.append((noop.get("rc") == 0 and noop.get("action") == "noop"
                    and "level.update" not in r_noop.tags,
                    "LEVEL3b 已是目标等级 => noop（不打 UPDATE）"))

        r_bad = _R(verify_level=42)
        bad = run_level_fixture(r_bad, "csharp", prof, sc, 43, {}, tmp)
        out.append((bad.get("rc") == 2 and "读回与目标不符" in (bad.get("error") or ""),
                    "LEVEL 牙4 写后读回不符 => rc=2"))

        r_restore_bad = _R(restore_ok=False)
        res_rb = run_level_fixture(r_restore_bad, "csharp", prof, sc, 43, {}, tmp)
        err = restore_level_fixture(r_restore_bad, res_rb, {})
        out.append((err is not None and "还原后 Level" in err,
                    "LEVEL 牙5 还原后重读不符 => 返回错误（调用方升级 rc=2）"))
    finally:
        (globals()["_check_db_pairing"], globals()["placement_envir_root"],
         globals()["placement_characters_db"], globals()["placement_db_client"]) = orig

    import inspect
    src = inspect.getsource(run_side)
    out.append(("run_level_fixture(" in src and "restore_level_fixture(" in src
                and src.index("run_level_fixture(") < src.index('"stack"'),
                "LEVEL4 接线：run_side 起栈前应用 level，finally 还原"))
    return out


def _selftest_postrun_symmetry(tmp):
    """卡 openmir2-postrun-symmetry-drift：跑后漂移只标注、列出两侧 sha。"""
    out = []
    base = os.path.join(tmp, "postrun")
    roots = {
        "csharp": os.path.join(base, "csharp-root", "M2GameSvr"),
        "csharp-hook": os.path.join(base, "csharp-root", "M2GameSvr-hook"),
        "rust": os.path.join(base, "rust-root", "M2GameSvr"),
    }

    def write(root, rel, data):
        path = os.path.join(root, *rel.split("/"))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(data)

    for root in roots.values():
        write(root, "Castle/List.txt", b"0\r\n1\r\n")
        write(root, "Castle/0/SabukW.txt", b"[Setup]\r\nTotalGold=0\r\n")
        write(root, "Envir/A.txt", b"alpha")
    prof = {"sides": {"csharp": {"envir_root": roots["csharp"]},
                      "rust": {"envir_root": roots["rust"]}}}
    out.append((postrun_symmetry(prof) == [],
                "POSTRUN1 三方一致 => 空列表（无漂移）"))
    write(roots["csharp-hook"], "Castle/List.txt", b"0\r\n")
    drift = postrun_symmetry(prof)
    entry = next((d for d in drift if d.get("file") == "Castle/List.txt"), {})
    out.append((len(drift) == 1 and set(entry.get("sha256") or {})
                == {"csharp", "csharp-hook", "rust"},
                "POSTRUN 牙1 影子 List 漂移 => 1 项，列出三方 sha"))
    write(roots["csharp-hook"], "Castle/List.txt", b"0\r\n1\r\n")
    write(roots["rust"], "Envir/A.txt", b"alpha-drift")
    drift = postrun_symmetry(prof)
    out.append((any(d.get("file") == "Envir/A.txt" for d in drift),
                "POSTRUN 牙2 Envir 文件漂移 => 报 Envir/A.txt"))
    write(roots["rust"], "Envir/A.txt", b"alpha")
    write(roots["csharp"], "Castle/0/SabukW.txt", b"[Setup]\r\nTotalGold=500\r\n")
    drift = postrun_symmetry(prof)
    out.append((any(d.get("file") == "Castle/0/SabukW.txt" for d in drift),
                "POSTRUN 牙3 SabukW 漂移 => 报 Castle/0/SabukW.txt"))
    import inspect
    src = inspect.getsource(execute)
    i_ok = src.index('summary["exit_code"] = 0 if ok else 2')
    i_drift = src.index('summary["post_run_symmetry_drift"] = drift')
    i_art = src.index("artifacts_ok = write_artifacts(")
    out.append(("post_run_symmetry_drift" in src and i_ok < i_drift < i_art,
                "POSTRUN2 接线：execute 在 verdict 定稿后、写产物前复核并写 post_run_symmetry_drift"))
    return out


def _selftest_inventory_fixture(tmp):
    """卡 openmir2-window-fixture-inventory：replace/drop 声明、写读回、回滚、还原。"""
    import re as _re
    out = []

    # ---- 声明期 ----
    raw = {"inventory_replace": [
               {"position": 40, "index": 207, "make_index": 5001,
                "std_mode": 5, "ani_count": 0, "dura": 10, "dura_max": 20},
               {"position": 41, "index": 68, "make_index": 5003}],
           "inventory_drop": [42, {"position": 43, "note": "清空"}]}
    decl = _validate_fixture_decl(dict(raw), "selftest.fixtures")
    out.append(([it["position"] for it in decl["inventory_replace"]] == [40, 41]
                and [it["position"] for it in decl["inventory_drop"]] == [42, 43]
                and decl["inventory_replace"][0]["dura"] == 10
                and decl["inventory_drop"][1]["note"] == "清空",
                "INV1 声明解析：replace 2 件 + drop 2 槽（int 与对象两种写法）"))

    bad_ok = True
    for bad in ({"inventory_replace": []},
                {"inventory_replace": [{"position": 0, "index": 1}]},
                {"inventory_replace": [{"position": 0, "index": 1, "make_index": 1,
                                        "bogus": 1}]},
                {"inventory_replace": [{"position": 46, "index": 1, "make_index": 1}]},
                {"inventory_replace": [{"position": 0, "index": 1, "make_index": 0}]},
                {"inventory_replace": [{"position": 0, "index": 1, "make_index": 1},
                                       {"position": 0, "index": 2, "make_index": 2}]},
                {"inventory_drop": []},
                {"inventory_drop": [46]},
                {"inventory_drop": ["3"]},
                {"inventory_drop": [{"slot": 3}]},
                {"inventory_replace": [{"position": 5, "index": 1, "make_index": 1}],
                 "inventory_drop": [5]}):
        try:
            _validate_fixture_decl(dict(bad), "x")
            bad_ok = False
        except ConfigError:
            pass
    out.append((bad_ok, "INV 牙1 空数组/缺键/未知键/越界/make_index=0/同槽重复/"
                        "replace 与 drop 重叠 => ConfigError"))

    # ---- 对称性（两侧声明必须逐字节相同） ----
    d1 = _validate_fixture_decl(dict(raw), "a")
    d2 = _validate_fixture_decl(
        {"inventory_replace": [{"position": 40, "index": 207, "make_index": 9999}]}, "b")
    try:
        _check_fixture_symmetry({"csharp": d1, "rust": d2})
        sym_ok = False
    except ConfigError as ex:
        sym_ok = "不一致" in str(ex) and "inventory_replace" in str(ex)
    out.append((sym_ok, "INV 牙2 两侧 inventory_replace 不同 => ConfigError（对拍无效）"))
    try:
        _check_fixture_symmetry({"csharp": d1, "rust": _validate_fixture_decl(
            dict(raw), "a2")})
        same_ok = True
    except ConfigError:
        same_ok = False
    out.append((same_ok, "INV2b 两侧声明相同 => 通过"))

    # ---- SQL 构造/解析 ----
    w = _inv_write_sql("mir2_test_db", 6228, {"op": "replace", "position": 40,
                                             "make_index": 5001, "std_index": 207,
                                             "dura": 10, "dura_max": 20})
    wd = _inv_write_sql("mir2_test_db", 6228, {"op": "drop", "position": 42})
    rs = _inv_restore_sql("mir2_test_db", 6228, 40,
                          {"Id": 9, "Position": 40, "MakeIndex": 5001,
                           "StdIndex": 207, "Dura": 10, "DuraMax": 20})
    rs_empty = _inv_restore_sql("mir2_test_db", 6228, 42, None)
    out.append(("INSERT INTO" in w and "`Position`, `MakeIndex`" in w
                and "DELETE FROM" in wd and "INSERT INTO" not in wd
                and "`Id`, `PlayerId`" in rs and "9, 6228, 40" in rs
                and "INSERT INTO" not in rs_empty,
                "INV3 SQL 形态：replace=DELETE+INSERT，drop 只 DELETE，还原连 Id 写回"))
    dup_ok = True
    for bad_rows in ("1\t2\t3\t4\t5\n", "0\t1\t2\t3\t4\t5\n0\t6\t2\t3\t4\t5\n",
                     "0\t1\t2\t3\t4\tNULL\n"):
        try:
            _parse_inv_rows(bad_rows)
            dup_ok = False
        except ConfigError:
            pass
    out.append((dup_ok, "INV 牙3 行读回列数不对/同槽多行/NULL => ConfigError"))

    # ---- 运行期：有状态假栈（模拟 characters_bagitem） ----
    class _Bag:
        def __init__(self):
            self.bag = {p: {"Id": 100 + p, "Position": p, "MakeIndex": 9000 + p,
                            "StdIndex": 0, "Dura": 0, "DuraMax": 0}
                        for p in range(46)}
            self.next_id = 900
            self.tags = []
            self.fail_verify_pos = None
            self.drift_after_write = False
            self.written = 0

        def _rows(self, positions):
            return "".join("%d\t%d\t%d\t%d\t%d\t%d\n"
                           % (p, r["Id"], r["MakeIndex"], r["StdIndex"],
                              r["Dura"], r["DuraMax"])
                           for p, r in sorted(self.bag.items())
                           if p in positions)

        def _fp(self, excluded):
            rows = [r for p, r in sorted(self.bag.items()) if p not in excluded]
            blob = "|".join("%d:%d:%d:%d:%d" % (r["Position"], r["MakeIndex"],
                                                r["StdIndex"], r["Dura"], r["DuraMax"])
                            for r in rows)
            h = hashlib.md5(blob.encode("utf-8")).hexdigest()
            if self.drift_after_write and self.written:
                h = "deadbeef" * 4
            return "%d\t%s\n" % (len(rows), h)

        def _apply_write(self, pos, text):
            self.written += 1
            self.bag.pop(pos, None)
            m = _re.search(r"VALUES \((\d+), (\d+), (\d+), (\d+), (\d+), (\d+)\)\s*;\s*$",
                           text)
            if m:
                _pid, p, mk, idx, dura, dmax = (int(x) for x in m.groups())
                self.bag[p] = {"Id": self.next_id, "Position": p, "MakeIndex": mk,
                               "StdIndex": idx, "Dura": dura, "DuraMax": dmax}
                self.next_id += 1

        def _apply_restore(self, pos, text):
            self.bag.pop(pos, None)
            m = _re.search(
                r"VALUES \((\d+), (\d+), (\d+), (\d+), (\d+), (\d+), (\d+)\)\s*;\s*$",
                text)
            if m:
                rid, _pid, p, mk, idx, dura, dmax = (int(x) for x in m.groups())
                self.bag[p] = {"Id": rid, "Position": p, "MakeIndex": mk,
                               "StdIndex": idx, "Dura": dura, "DuraMax": dmax}

        def run(self, side, tag, argv, env=None, timeout=120, stdin_text=None,
                capture=True):
            self.tags.append(tag)
            text = stdin_text or ""
            if tag == "inventory_fixture.character.columns":
                return 0, "Id\nChrName\nLoginID\nDeleted\n", ""
            if tag == "inventory_fixture.character":
                return 0, "1\t6228\t0\n", ""
            if tag == "inventory_fixture.columns":
                return 0, "Id\nPlayerId\nPosition\nMakeIndex\nStdIndex\nDura\nDuraMax\n", ""
            if tag == "inventory_fixture.read":
                return 0, self._fp(self.excluded), ""
            if tag == "inventory_fixture.verify":
                return 0, self._fp(self.excluded), ""
            if tag == "inventory_fixture.snapshot":
                m = _re.search(r"IN \(([^)]*)\)", text)
                positions = {int(x) for x in m.group(1).split(",")} if m else set()
                return 0, self._rows(positions), ""
            if tag == "inventory_fixture.template":
                return 0, "5\t0\n", ""
            m = _re.match(r"inventory_fixture\.(write|verify|restore|restore_verify|"
                          r"rollback|rollback_verify)_(\d+)$", tag)
            if m:
                kind, pos = m.group(1), int(m.group(2))
                if kind == "write":
                    self._apply_write(pos, text)
                elif kind == "restore":
                    self._apply_restore(pos, text)
                elif kind == "rollback":
                    self._apply_restore(pos, text)
                if kind in ("verify",):
                    if self.fail_verify_pos == pos:
                        r = dict(self.bag[pos])
                        r["MakeIndex"] = 1
                        return 0, "%d\t%d\t%d\t%d\t%d\t%d\n" % (
                            pos, r["Id"], r["MakeIndex"], r["StdIndex"],
                            r["Dura"], r["DuraMax"]), ""
                return 0, self._rows({pos}), ""
            return 1, "", "unexpected tag %s" % tag

    class _Dry:
        dry_run = True

        def __init__(self):
            self.lines = []
            self.calls = []

        def log(self, t):
            self.lines.append(t)

        def run(self, *a, **k):
            self.calls.append(a)
            return 0, "", ""

    prof = {"char_name": "winplay0", "account": "winplay0",
            "sides": {"csharp": {"db": {"characters_db": "mir2_test_db"}}}}
    sc = prof["sides"]["csharp"]
    orig = (globals()["_check_db_pairing"], globals()["placement_envir_root"],
            globals()["placement_characters_db"], globals()["placement_db_client"])
    globals()["_check_db_pairing"] = lambda *a, **k: None
    globals()["placement_envir_root"] = lambda *a, **k: tmp
    globals()["placement_characters_db"] = lambda side, sc_: "mir2_test_db"
    globals()["placement_db_client"] = lambda prof_, side_: {
        "mysql": [DEFAULT_MYSQL], "host": "127.0.0.1", "port": 3306,
        "user": "root", "password": "SECRETPW"}

    def make_bag():
        b = _Bag()
        b.excluded = {40, 41, 42, 43}
        return b

    try:
        dr = _Dry()
        dres = run_inventory_fixture(dr, "csharp", prof, sc, decl, {}, tmp)
        out.append((dres.get("rc") == 0 and dres.get("action") == "dry-run"
                    and any("dry-run 不连库" in l for l in dr.lines)
                    and any("replace Position=40" in l for l in dr.lines)
                    and any("drop    Position=42" in l for l in dr.lines)
                    and not dr.calls,
                    "INV4 dry-run：打印 replace/drop 预览且不执行任何 mysql"))

        b = make_bag()
        before = {p: dict(r) for p, r in b.bag.items()}
        res = run_inventory_fixture(b, "csharp", prof, sc, decl, {}, tmp)
        ev = level_evidence(res)
        blob = json.dumps(ev, ensure_ascii=False)
        applied_ok = (res.get("rc") == 0 and res.get("action") == "applied"
                      and b.bag[40]["MakeIndex"] == 5001
                      and b.bag[40]["StdIndex"] == 207
                      and 41 in b.bag and 42 not in b.bag
                      and "SECRETPW" not in blob and "_session" not in ev
                      and "_session" in res)
        out.append((applied_ok,
                    "INV5 生产路径：40/41 被替换、42 被 drop；证据不含凭据（_session 只在内存）"))
        err = restore_inventory_fixture(b, res, {})
        out.append((err is None and b.bag == before,
                    "INV6 还原：按快照整行写回（含 Id），背包与写前逐字段相同"))

        b = make_bag()
        b.excluded = {7}  # 只 drop 7 ⇒ 指纹排除集必须跟着声明走
        orig_bag = {p: dict(r) for p, r in b.bag.items()}
        res_drop = run_inventory_fixture(
            b, "csharp", prof, sc,
            _validate_fixture_decl({"inventory_drop": [7]}, "x"), {}, tmp)
        dropped = 7 not in b.bag  # 还原前先取证（还原后该行会回来）
        err_drop = restore_inventory_fixture(b, res_drop, {})
        out.append((res_drop.get("rc") == 0 and dropped
                    and err_drop is None and b.bag == orig_bag,
                    "INV7 drop 单槽：写后该槽为空，还原后整行（含 Id）复原"))

        b = make_bag()
        b.fail_verify_pos = 41
        orig_bag = {p: dict(r) for p, r in b.bag.items()}
        res_fail = run_inventory_fixture(b, "csharp", prof, sc, decl, {}, tmp)
        out.append((res_fail.get("rc") == 2
                    and "读回" in (res_fail.get("error") or "")
                    and res_fail.get("rollback_error") is None
                    and b.bag == orig_bag,
                    "INV 牙4 写后读回不符 => rc=2，且已写项就地回滚到写前"))

        b = make_bag()
        b.drift_after_write = True
        res_fp = run_inventory_fixture(b, "csharp", prof, sc, decl, {}, tmp)
        out.append((res_fp.get("rc") == 2 and "指纹变了" in (res_fp.get("error") or ""),
                    "INV 牙5 其它槽位指纹变化 => rc=2（动到了未声明的行）"))

        b = make_bag()
        empty_drop = _validate_fixture_decl({"inventory_drop": [45]}, "x")
        del b.bag[45]
        res_empty = run_inventory_fixture(b, "csharp", prof, sc, empty_drop, {}, tmp)
        out.append((res_empty.get("rc") == 2
                    and "本来为空" in (res_empty.get("error") or ""),
                    "INV 牙6 drop 空槽 => rc=2（夹具没生效，不许假绿）"))

        b_bad_tpl = _validate_fixture_decl(
            {"inventory_replace": [{"position": 3, "index": 999999,
                                    "make_index": 1}]}, "x")

        class _NoTpl(_Bag):
            def run(self, side, tag, argv, env=None, timeout=120, stdin_text=None,
                    capture=True):
                if tag == "inventory_fixture.template":
                    return 0, "", ""
                return _Bag.run(self, side, tag, argv, env=env, timeout=timeout,
                                stdin_text=stdin_text, capture=capture)

        nb = _NoTpl()
        nb.excluded = {3}
        nb.bag = {3: {"Id": 1, "Position": 3, "MakeIndex": 9003, "StdIndex": 0,
                      "Dura": 0, "DuraMax": 0}}
        res_tpl = run_inventory_fixture(nb, "csharp", prof, sc, b_bad_tpl, {}, tmp)
        out.append((res_tpl.get("rc") == 2 and "模板" in (res_tpl.get("error") or "")
                    and not any(t.startswith("inventory_fixture.write_") for t in nb.tags),
                    "INV 牙7 模板不存在 => rc=2，且一个写语句都没发（先校验后写）"))

        b = make_bag()
        mm = _validate_fixture_decl(
            {"inventory_replace": [{"position": 3, "index": 207, "make_index": 1,
                                    "std_mode": 9}]}, "x")
        res_mm = run_inventory_fixture(b, "csharp", prof, sc, mm, {}, tmp)
        out.append((res_mm.get("rc") == 2
                    and "StdMode" in (res_mm.get("error") or ""),
                    "INV 牙8 声明 std_mode 与模板不符 => rc=2"))
    finally:
        (globals()["_check_db_pairing"], globals()["placement_envir_root"],
         globals()["placement_characters_db"], globals()["placement_db_client"]) = orig

    import inspect
    src = inspect.getsource(run_side)
    i_fix = src.index("run_inventory_fixture(")
    i_stack = src.index('stack_argv = expand_argv(sc["stack"], ctx)')
    i_restore = src.index("restore_inventory_fixture(")
    out.append((i_fix < i_stack < i_restore,
                "INV8 接线：run_side 起栈前应用、finally 停栈后还原"))
    return out


def _selftest_monsters_prepend(tmp):
    """卡 openmir2-fixture-monsters-prepend：前插布局（含反 append 牙）+ RNG 台账裁剪。"""
    out = []
    lines = ["3\t330\t331\t鸡\t0\t1\t360"]
    body = "3\t294\t341\t神鹰\t10\t1\t360\r\n3\t295\t341\t鹿\t10\t1\t360\r\n"
    raw = body.encode("gbk")
    block = "".join(ln + "\r\n" for ln in lines).encode("gbk")

    ok, why = _mongen_fixture_layout(raw, block + raw, lines)
    out.append((ok and "prepend" in why,
                "MERGE1 前插布局 => 机判通过（夹具行在头，原内容字节后移）"))
    ok, why = _mongen_fixture_layout(raw, raw + block, lines)
    out.append((not ok and "append" in why,
                "MERGE 牙1 追加布局 => 机判红（点名 append：200ms/条，窗口内轮不到）"))
    ok, why = _mongen_fixture_layout(raw, raw[:10] + block + raw[10:], lines)
    out.append((not ok and "不在文件头部" in why,
                "MERGE 牙2 插在文件中间 => 机判红"))
    ok, why = _mongen_fixture_layout(raw, block + raw[:-1], lines)
    out.append((not ok and "字节不一致" in why,
                "MERGE 牙3 前插但原内容被改一个字节 => 机判红"))
    ok, why = _mongen_fixture_layout(raw, raw, lines)
    out.append((not ok and "没有写进" in why,
                "MERGE 牙4 什么都没写 => 机判红"))

    # 端到端：run_fixtures 写文件 → 头块断言 → restore 逐字节复原
    root = os.path.join(tmp, "merge-deploy")
    os.makedirs(os.path.join(root, "Envir"), exist_ok=True)
    with open(os.path.join(root, "Envir", "MapInfo.txt"), "wb") as f:
        f.write("[3 主城]\r\n".encode("gbk"))
    mp = os.path.join(root, "Envir", "MonGen.txt")
    with open(mp, "wb") as f:
        f.write(raw)
    side_dir = os.path.join(tmp, "merge-side")
    os.makedirs(side_dir, exist_ok=True)
    decl = _validate_fixture_decl(
        {"monsters": [{"map": "3", "x": 330, "y": 331, "name": "鸡"}]}, "x")
    runner = Runner(tmp, None, dry_run=False)
    res = run_fixtures(runner, "rust", {}, {"envir_root": root}, decl, {}, side_dir)
    after = open(mp, "rb").read()
    out.append((res.get("rc") == 0 and res.get("action") == "prepended"
                and res.get("prepend") == lines
                and res["targets"][0].get("layout") == "prepend"
                and after.startswith(("3\t330\t331\t鸡\t0\t1\t360\r\n").encode("gbk")),
                "MERGE2 端到端：MonGen.txt 头部即夹具行、tail 为原文件字节未变、"
                "action=prepended"))
    before_sha = hashlib.sha256(raw).hexdigest()
    err = restore_fixtures(res)
    out.append((err is None and open(mp, "rb").read() == raw
                and hashlib.sha256(open(mp, "rb").read()).hexdigest() == before_sha,
                "MERGE3 restore 逐字节复原（sha256 与写前一致）"))

    # RNG 台账裁剪
    good = (b"INJECT_OK 42\n" + b"SEED_PROBE 1002\n"
            + b"".join(b"%d\tmovr\t10\t%d\tsite\n" % (i, i) for i in range(1, 200)))
    small = os.path.join(tmp, "rng-small.tsv")
    with open(small, "wb") as f:
        f.write(good)
    st = trim_rng_ledger(small, max_bytes=10 * 1024 * 1024, head_lines=10)
    out.append((st["trimmed"] is False and st["original_lines"] == 201
                and st["original_bytes"] == len(good)
                and st["original_sha256"] == hashlib.sha256(good).hexdigest()
                and open(small, "rb").read() == good,
                "MERGE4 未超限 => 不裁剪，但仍记录 size/行数/sha256"))

    big = os.path.join(tmp, "rng-big.tsv")
    with open(big, "wb") as f:
        f.write(good)
    st = trim_rng_ledger(big, max_bytes=100, head_lines=3)
    head = b"".join(good.splitlines(True)[:3])
    ok_head, detail = check_rng_ledger(big)
    out.append((st["trimmed"] is True and st["kept_lines"] == 3
                and st["kept_bytes"] == len(head)
                and st["original_lines"] == 201
                and st["original_sha256"] == hashlib.sha256(good).hexdigest()
                and open(big, "rb").read() == head
                and ok_head,
                "MERGE5 超限 => 只留前 3 行 + 原始 size/行数/sha256；head 仍过账本校验"
                "（%s）" % detail))
    miss = trim_rng_ledger(os.path.join(tmp, "rng-missing.tsv"))
    out.append((miss["trimmed"] is False and "不存在" in (miss.get("error") or ""),
                "MERGE 牙5 台账不存在 => 记 error、不抛异常"))

    # review F1：布局机判失败 => 文件必须按备份回滚（不得静默残留）
    root2 = os.path.join(tmp, "merge-fail")
    os.makedirs(os.path.join(root2, "Envir"), exist_ok=True)
    with open(os.path.join(root2, "Envir", "MapInfo.txt"), "wb") as f:
        f.write(("[3 主城]" + "\r\n").encode("gbk"))
    mp2 = os.path.join(root2, "Envir", "MonGen.txt")
    with open(mp2, "wb") as f:
        f.write(raw)
    side2 = os.path.join(tmp, "merge-fail-side")
    os.makedirs(side2, exist_ok=True)
    orig_layout = globals()["_mongen_fixture_layout"]
    globals()["_mongen_fixture_layout"] = lambda *a, **k: (False, "模拟机判失败")
    try:
        res_bad = run_fixtures(Runner(tmp, None, dry_run=False), "rust", {},
                               {"envir_root": root2}, decl, {}, side2)
    finally:
        globals()["_mongen_fixture_layout"] = orig_layout
    out.append((res_bad.get("rc") == 2 and open(mp2, "rb").read() == raw
                and bool(res_bad.get("targets"))
                and res_bad["targets"][0].get("backup") is None
                and res_bad["targets"][0].get("action") == "restored",
                "MERGE 牙6 review-F1：布局机判失败 => rc=2 且文件按备份回滚为写前字节"
                "（targets 非空、backup 已清、action=restored）"))

    # review F1（reviewer-21 同结论）：把**写盘实现**变异成 append ⇒ rc=2 且文件字节不变
    root3 = os.path.join(tmp, "merge-append-mut")
    os.makedirs(os.path.join(root3, "Envir"), exist_ok=True)
    with open(os.path.join(root3, "Envir", "MapInfo.txt"), "wb") as f:
        f.write(("[3 主城]").encode("gbk"))
    mp3 = os.path.join(root3, "Envir", "MonGen.txt")
    with open(mp3, "wb") as f:
        f.write(raw)
    side3 = os.path.join(tmp, "merge-append-mut-side")
    os.makedirs(side3, exist_ok=True)
    orig_write = globals()["_mongen_write_layout"]

    def _append_write(path, body, block):
        with open(path, "wb") as f:
            f.write(body + block)  # 变异：append（窗口内轮不到）

    globals()["_mongen_write_layout"] = _append_write
    try:
        res_mut = run_fixtures(Runner(tmp, None, dry_run=False), "rust", {},
                               {"envir_root": root3}, decl, {}, side3)
    finally:
        globals()["_mongen_write_layout"] = orig_write
    mut_bytes = open(mp3, "rb").read()
    out.append((res_mut.get("rc") == 2
                and "append 布局" in (res_mut.get("error") or "")
                and mut_bytes == raw
                and bool(res_mut.get("targets"))
                and res_mut["targets"][0].get("action") == "restored",
                "MERGE 牙7 把写盘变异成 append => rc=2（点名 append 布局）且文件字节"
                "回滚为写前（不留半改状态）"))

    # review F2：栈没停成不许裁台账
    ok_guard = (rng_ledger_trim_allowed(True, 0) is True
                and rng_ledger_trim_allowed(True, 2) is False
                and rng_ledger_trim_allowed(True, None) is False
                # 没起过栈 => 没有写入者，裁剪安全（stop_ok 的既定语义）
                and rng_ledger_trim_allowed(False, None) is True)
    import inspect as _inspect_rs
    src_rs = _inspect_rs.getsource(run_side)
    out.append((ok_guard
                and "rng_ledger_trim_allowed(stack_attempted" in src_rs,
                "MERGE7 review-F2：裁剪守卫——只有 stop_ok(stack_attempted, stop_rc) "
                "才裁（栈没停成 => 不裁）"))

    import inspect
    src = inspect.getsource(run_side)
    i_stop = src.index('res["stop_rc"] = runner.run(side, "stop"')
    i_trim = src.index("trim_rng_ledger(res[\"rng_log\"])")
    out.append((i_stop < i_trim,
                "MERGE6 接线：停栈之后才裁剪台账（进程退出前台账仍在增长）"))
    return out


def _selftest_save_settle():
    """卡 `openmir2-harness-save-time-symmetry`：存档落定等待（离线，不连库）。

    * 配置 fail-closed（未知键 / window>5000 / quiet>window / poll<50 / bool 非 bool）；
    * 静默判定：恒定或「变化后静默」=> settled；持续变化或静默不足 => 未落定；
    * wait_save_settled（注入 sql_fn + 假时钟）：落定 rc=0 / 窗口内不落定 rc=2 save_unsettled /
      DB 错误 rc=2 / disabled 不连库 / require_changes 且零变化 => rc=2；
    * **schema 牙**：指纹 SQL 里的标识符必须都在 `sql/mir2_db.sql` 解析出的
      `characters` ∪ `characters_ablity` 列集里（review `b48197f6` F1：曾写
      `characters.ModifyTime`，该列不存在）；
    * 源码级接线：run_side 在「# 7) 抓库」之前调用。
    """
    out = []
    out.append((profile_save_settle({}) == {"enabled": True, "window_ms": 5000,
                                           "quiet_ms": 800, "poll_ms": 200,
                                           "require_changes": True},
                "SETTLE1 缺省配置（enabled/window=5000/quiet=800/poll=200/require_changes=True）"))
    out.append((profile_save_settle({"save_settle": {"window_ms": 3000}})["window_ms"] == 3000,
                "SETTLE2 顶层 save_settle 可覆盖"))
    prof_side = {"save_settle": {"window_ms": 3000},
                 "sides": {"rust": {"save_settle": {"enabled": False}}}}
    out.append((profile_save_settle(prof_side, "rust")["enabled"] is False
                and profile_save_settle(prof_side, "csharp")["window_ms"] == 3000,
                "SETTLE3 sides.<side>.save_settle 覆盖本侧"))

    def bad(decl, needle):
        try:
            profile_save_settle({"save_settle": decl})
            return False
        except ConfigError as e:
            return needle in str(e)

    out.append((bad({"bogus": 1}, "未知键"), "SETTLE 牙1 未知键 => 拒"))
    out.append((bad({"window_ms": 6000}, "越界"), "SETTLE 牙2 window_ms>5000 => 拒"))
    out.append((bad({"quiet_ms": 900, "window_ms": 800}, "quiet_ms"),
                "SETTLE 牙3 quiet_ms>window_ms => 拒"))
    out.append((bad({"poll_ms": 10}, "越界"), "SETTLE 牙4 poll_ms<50 => 拒"))
    out.append((bad({"enabled": "yes"}, "bool"), "SETTLE 牙5 enabled 非 bool => 拒"))
    out.append((bad({"require_changes": "yes"}, "bool"),
                "SETTLE 牙6 require_changes 非 bool => 拒"))

    def mk(seq):
        return [(i * 200, fp) for i, fp in enumerate(seq)]

    out.append((settle_decision(mk(["a"] * 6), 800)[0] is True,
                "SETTLE4 恒定指纹 >= quiet(800) => settled"))
    out.append((settle_decision(mk(["a"] * 6), 1200)[0] is False,
                "SETTLE5 静默跨度不足（1000<1200）=> 未落定"))
    out.append((settle_decision(mk(["a", "a", "b", "b", "b", "b", "b"]), 800)[0] is True,
                "SETTLE6 变化后静默 => settled（changes 计入）"))
    out.append((settle_decision(mk(["a", "a", "a", "b"]), 400)[0] is False,
                "SETTLE6b 新变化撤销此前 quiet：当前尾段尚未静默 => 不 settled"))
    out.append((settle_decision(mk(["a", "a", "a", "b", "b"]), 400)[0] is False,
                "SETTLE6b-ii 新指纹尾段仅 200ms<quiet(400)：旧 quiet 不得带入新尾段"))
    out.append((settle_decision(mk(list("abcdefg")), 800)[0] is False,
                "SETTLE7 持续变化 => 未落定"))

    class _R(object):
        dry_run = False

        def log(self, *_a, **_k):
            pass

    prof = {"char_name": "winplay0"}
    sc = {"db": {"characters_db": "mir2_test_db"}}
    settle = {"enabled": True, "window_ms": 1200, "quiet_ms": 400, "poll_ms": 200}

    def make_clock():
        t = {"ms": 0}

        def clk():
            v = t["ms"]
            t["ms"] += 200
            return v
        return clk

    seq = iter(["A\n46", "A\n46", "B\n46", "B\n46", "B\n46", "B\n46"])
    res = wait_save_settled(_R(), "csharp", prof, sc, settle, {},
                            sql_fn=lambda _s: next(seq), clock=make_clock())
    out.append((res["rc"] == 0 and res["settled"] is True and res["changes"] == 1
                and res["action"] == "settled",
                "SETTLE8 注入 sql：变化后静默 => rc=0 settled changes=1"))

    counter = {"n": 0}

    def always(_s):
        counter["n"] += 1
        return "X%d\n46" % counter["n"]

    res2 = wait_save_settled(_R(), "rust", prof, sc, settle, {},
                             sql_fn=always, clock=make_clock())
    out.append((res2["rc"] == 2 and "save_unsettled" in (res2["error"] or "")
                and len(res2["samples"]) >= 2,
                "SETTLE9 窗口内不落定 => rc=2 save_unsettled（含采样）"))

    def boom(_s):
        raise ConfigError("mysql down")

    res3 = wait_save_settled(_R(), "rust", prof, sc, settle, {},
                             sql_fn=boom, clock=make_clock())
    out.append((res3["rc"] == 2 and "save_unsettled" in (res3["error"] or ""),
                "SETTLE10 DB 错误 => rc=2 save_unsettled（fail-closed）"))

    res4 = wait_save_settled(_R(), "rust", prof, sc, dict(settle, enabled=False), {},
                             sql_fn=boom)
    out.append((res4["rc"] == 0 and res4["action"] == "disabled",
                "SETTLE11 enabled=false => 不连库、rc=0"))

    import inspect
    src = inspect.getsource(run_side)
    out.append(("wait_save_settled(" in src
                and src.index("wait_save_settled(") < src.index("# 7) 抓库"),
                "SETTLE12 接线：run_side 在「# 7) 抓库」之前调用 wait_save_settled"))

    # --- schema 牙（review `b48197f6` F1；两颗表都验）--------------------------
    cols = _schema_columns("characters")
    cols_a = _schema_columns("characters_ablity")
    sql = _save_settle_sql("mir2_test_db", "winplay0")
    idents = _sql_identifiers(sql)
    allowed = cols | cols_a | {"characters", "characters_ablity", "mir2_test_db"}
    out.append((len(cols) >= 30 and len(cols_a) >= 5
                and "LASTUPDATE" in cols and "ModifyTime" not in cols
                and "ModifyTime" in cols_a and "PlayerId" in cols_a
                and {"LASTUPDATE", "ModifyTime", "ChrName"} <= idents
                and idents <= allowed,
                "SETTLE13 指纹 SQL 标识符 ⊆ 冻结 schema 列集（characters ∪ characters_ablity"
                " + 库/表名）：%s" % sorted(idents - allowed)))
    out.append(("characters_ablity" in sql and "LEFT JOIN" in sql
                and "MAX(a.`ModifyTime`)" in sql,
                "SETTLE13b 指纹带第二见证：LEFT JOIN characters_ablity 取 MAX(a.ModifyTime)"))
    bad_sql = "SELECT MAX(`Nonsense`), COUNT(*) FROM `mir2_test_db`.`characters`;"
    out.append((not (_sql_identifiers(bad_sql) <= allowed),
                "SETTLE14 牙7 同一判据对不存在的列 Nonsense 判红（牙本身不恒绿）"))

    # --- require_changes（review `b48197f6` F2②）：静默 != 存档已触发 ----------
    fixed = {"n": 0}

    def constant(_s):
        fixed["n"] += 1
        return "A\n46"

    ss = dict(settle, require_changes=True)
    res5 = wait_save_settled(_R(), "csharp", prof, sc, ss, {},
                             sql_fn=constant, clock=make_clock())
    # require_changes=true 且零变化时 quiet 不提前收口，必须观满 window；
    # 报错串用**实际 elapsed**，不能用较小的 settle 时刻冒充本次零变化观测跨度。
    err5 = res5["error"] or ""
    out.append((res5["rc"] == 2 and res5["action"] == "no_change_observed"
                and res5["settled"] is True and res5["changes"] == 0
                and "no_change_observed" in err5
                and "静默 != 存档已触发" in err5
                and ("实际观测的 %dms" % res5["elapsed_ms"]) in err5
                and res5["elapsed_ms"] >= 1200
                and len(res5["samples"]) >= 6
                and "window 上限 1200ms" in err5,
                "SETTLE15 牙8 require_changes 且零变化 => 观满 window 后 rc=2 no_change_observed"
                "（quiet 不提前截断；实际 elapsed=%dms）" % res5["elapsed_ms"]))
    delayed_b = "B" + chr(10) + "46"
    seq_delayed = iter([constant(None), constant(None), constant(None),
                        delayed_b, delayed_b, delayed_b, delayed_b])
    res_delayed = wait_save_settled(_R(), "csharp", prof, sc, ss, {},
                                    sql_fn=lambda _s: next(seq_delayed), clock=make_clock())
    delayed_samples = res_delayed["samples"]
    delayed_change_times = [cur["t_ms"] for prev, cur in zip(delayed_samples, delayed_samples[1:])
                            if cur["sha256_12"] != prev["sha256_12"]]
    delayed_tail_quiet_ms = (delayed_samples[-1]["t_ms"] - delayed_change_times[-1]
                             if delayed_samples and delayed_change_times else None)
    out.append((res_delayed["rc"] == 0 and res_delayed["action"] == "settled"
                and res_delayed["changes"] == 1 and len(delayed_samples) >= 5
                and res_delayed["elapsed_ms"] > 400
                and delayed_tail_quiet_ms is not None
                and delayed_tail_quiet_ms >= ss["quiet_ms"],
                "SETTLE15d 延迟落库正对照：quiet 后才变化 => 继续采样并 rc=0 changes=1；"
                "末样本-最后变化样本=%sms >= quiet" % delayed_tail_quiet_ms))
    seq2 = iter(["A\n46", "B\n46", "B\n46", "B\n46", "B\n46"])
    res6 = wait_save_settled(_R(), "csharp", prof, sc, ss, {},
                             sql_fn=lambda _s: next(seq2), clock=make_clock())
    out.append((res6["rc"] == 0 and res6["settled"] is True and res6["changes"] == 1,
                "SETTLE15b require_changes 但观测到变化 => rc=0 settled"))
    res7 = wait_save_settled(_R(), "csharp", prof, sc,
                             dict(settle, require_changes=False), {},
                             sql_fn=constant, clock=make_clock())
    out.append((res7["rc"] == 0 and res7["settled"] is True and res7["changes"] == 0,
                "SETTLE15c 显式 require_changes=false（只读场景逃生阀）=> 零变化也 rc=0"))

    # --- 边界语义（F5）：quiet_ms=0 约 poll_ms 后落定；window_ms=0 必 timeout ----
    b0 = wait_save_settled(_R(), "rust", prof, sc,
                           {"enabled": True, "window_ms": 1200, "quiet_ms": 0,
                            "poll_ms": 200, "require_changes": False}, {},
                           sql_fn=constant, clock=make_clock())
    b1 = wait_save_settled(_R(), "rust", prof, sc,
                           {"enabled": True, "window_ms": 0, "quiet_ms": 0,
                            "poll_ms": 200, "require_changes": False}, {},
                           sql_fn=constant, clock=make_clock())
    out.append((b0["rc"] == 0 and len(b0["samples"]) == 2 and b1["rc"] == 2
                and b1["action"] == "timeout" and len(b1["samples"]) == 1,
                "SETTLE16 边界：quiet_ms=0 => 第 2 次采样即落定；window_ms=0 => 必 timeout"))

    # --- 原始见证（真机轮次要能看出「变化来自哪一列 / 基线是不是 NULL / 是否同一秒」）---
    tab = chr(9)
    w_a = ("2026-10-25 09:00:00" + tab + "1" + tab + "2026-10-25 09:00:01")
    w_b = ("2026-10-25 09:05:00" + tab + "1" + tab + "2026-10-25 09:05:01")
    seqw = iter([w_a, w_b, w_b, w_b, w_b, w_b])
    resw = wait_save_settled(_R(), "csharp", prof, sc, settle, {},
                             sql_fn=lambda _s: next(seqw), clock=make_clock())
    out.append((resw["rc"] == 0 and resw["changes"] == 1
                and resw["samples"][0]["witness"] == w_a.split(tab)
                and resw["samples"][-1]["witness"] == w_b.split(tab)
                and resw["witness_first"] == w_a.split(tab)
                and resw["witness_last"] == w_b.split(tab)
                and "witness_error" not in resw["samples"][0],
                "SETTLE17 每条采样带原始见证 [lastupdate,count,abil_modifytime]，首末见证另有"
                " witness_first/witness_last"))
    # 空首列（LASTUPDATE 为 NULL）与空末列（无 characters_ablity 行）都必须拆得出空串列，
    # 否则真机碰到 NULL 只会看到 witness=null，看不出「是哪一列空」。
    null_first = tab + "1" + tab + "2026-10-25 09:00:01"
    seqn = iter([null_first] * 5)
    resn = wait_save_settled(_R(), "csharp", prof, sc,
                             dict(settle, require_changes=False), {},
                             sql_fn=lambda _s: next(seqn), clock=make_clock())
    out.append((resn["samples"][0].get("witness") == ["", "1", "2026-10-25 09:00:01"]
                and "witness_error" not in resn["samples"][0],
                "SETTLE17b 空首列（LASTUPDATE=NULL）=> witness ['', '1', ts]，不许退化成 null"))
    resx = wait_save_settled(_R(), "rust", prof, sc,
                             dict(settle, require_changes=False), {},
                             sql_fn=constant, clock=make_clock())
    bx = resx["samples"][0]
    out.append((bx.get("witness") is None and "witness_error" in bx
                and resx["rc"] == 0,
                "SETTLE18 见证解析不了时不改判据：witness=null + witness_error，判据仍是整行 sha256"))

    # --- 缺键必须落安全侧（reviewer-21 delta：不带 require_changes + 恒定指纹曾放行）-----
    resd = wait_save_settled(_R(), "csharp", prof, sc,
                             {"enabled": True, "window_ms": 1200, "quiet_ms": 400,
                              "poll_ms": 200}, {},
                             sql_fn=constant, clock=make_clock())
    out.append((resd["require_changes"] is True and resd["rc"] == 2
                and resd["action"] == "no_change_observed"
                and resd["changes"] == 0,
                "SETTLE19 牙9 settle 不带 require_changes（不经 profile 层）=> 默认 true、"
                "恒定指纹必须 rc=2 no_change_observed（不许静默降级放行）"))
    return out


def _falsy_or_exit_code_nodes(src):
    """AST 扫描：找出 `... exit_code ... or 2` 形态的 `BoolOp(Or)`（review F2）。

    文本匹配两头都坏（注释误报 / 换引号与分支后假绿），所以只认语法树：
    `Or` 的右值是常量 `2`、左值文本里出现 `exit_code`。
    """
    import ast
    out = []
    tree = ast.parse(src)
    for node in ast.walk(tree):
        if not (isinstance(node, ast.BoolOp) and isinstance(node.op, ast.Or)):
            continue
        vals = node.values
        right = vals[-1]
        if not (isinstance(right, ast.Constant) and right.value == 2):
            continue
        left_dump = " ".join(ast.dump(v) for v in vals[:-1])
        if "exit_code" in left_dump:
            out.append(node)
    return out


def _selftest_main_exit_code(tmp):
    """卡 openmir2-runbatch-rc-attribution：PASS=>0 / FAIL=>2 / DRY-RUN=>0 / None=>2。"""
    import inspect
    out = []
    out.append((main_exit_code({"exit_code": 0}) == 0,
                "RC1 PASS（exit_code=0）=> 进程 rc=0（旧实现 `0 or 2` 会返回 2）"))
    out.append((main_exit_code({"exit_code": 2}) == 2,
                "RC2 FAIL/UNDECIDABLE（exit_code=2）=> rc=2"))
    out.append((main_exit_code({}) == 2 and main_exit_code({"exit_code": None}) == 2,
                "RC3 缺 exit_code / None => rc=2（只有 None 才回落）"))
    src = inspect.getsource(main)
    out.append(("main_exit_code(summary)" in src
                and not _falsy_or_exit_code_nodes(src),
                "RC4 接线：main() 走 main_exit_code，且 AST 扫描无 `exit_code … or 2` 形态"))
    NL = "\n"
    comment_src = (
        "def f(summary):" + NL
        + "    # 历史写法 return int(summary.get('exit_code') or 2) 已废弃" + NL
        + "    return 0" + NL)
    out.append((_falsy_or_exit_code_nodes(comment_src) == [],
                "RC 牙1 AST 扫描不误报注释里的旧写法（文本匹配会红）"))
    bad_src = (
        "def f(summary):" + NL
        + "    if summary.get('exit_code') is not None:" + NL
        + "        return 0" + NL
        + "    return int(summary.get('exit_code') or 2)" + NL)
    out.append((len(_falsy_or_exit_code_nodes(bad_src)) == 1,
                "RC 牙1b AST 扫描能抓住「条件分支里保留 `or 2`」的换形（文本匹配会漏）"))
    out.append((main_exit_code({"exit_code": True}) == 2,
                "RC 牙1c bool True 不当作 0/1（fail-closed 到 2）"))
    out.append((main_exit_code({"exit_code": False}) == 2,
                "RC 牙1d **bool False 不得当成 0**（review F1：False 被 int() 会假绿）"))
    sql = _save_settle_sql("mir2_test_db", "winplay0")
    out.append(("c.`LASTUPDATE`" in sql and "characters_ablity" in sql
                and "characters`.`ModifyTime" not in sql
                and "MAX(`ModifyTime`)" not in sql,
                "RC 牙2 存档落定 SQL 用 characters.LASTUPDATE + characters_ablity.ModifyTime，"
                "且不出现 characters.ModifyTime（防旧版错列回归）"))
    return out


def selftest():
    tmp = tempfile.mkdtemp(prefix="card-window-selftest-")
    # **C3（review 62ac57e1）**：本函数里所有「拿生产根当靶」的用例（FX27 / FX-F13 / F14 / F17）
    # 一律改指**假根**——守卫坏掉时变异也只写 tmp，绝不可能写真实生产。
    # 「一个测试的安全性不能依赖被测代码是正确的」。
    _prod_root_saved = globals()["FIXTURE_PROD_ROOT"]
    _prod_mon_real = os.path.join(_prod_root_saved, "M2GameSvr", "Envir", "MonGen.txt")
    _prod_mon_sha0 = (hashlib.sha256(open(_prod_mon_real, "rb").read()).hexdigest()
                      if os.path.isfile(_prod_mon_real) else None)
    _prod_conf_real = os.path.join(_prod_root_saved, "M2GameSvr", "Server.conf")
    _prod_conf_sha0 = (hashlib.sha256(open(_prod_conf_real, "rb").read()).hexdigest()
                       if os.path.isfile(_prod_conf_real) else None)
    _prod_fake = os.path.join(tmp, "fake-prod-root")
    _prod_fake_env = os.path.join(_prod_fake, "M2GameSvr", "Envir")
    os.makedirs(_prod_fake_env, exist_ok=True)
    # G2（review 404c26b8）：假根内容**自己合成**（最小合法 MapInfo/MonGen），不从真实部署
    # 复制 —— 自测不该依赖部署存在，也不该把生产内容读进 temp。
    with open(os.path.join(_prod_fake_env, "MapInfo.txt"), "w", encoding="gbk") as _fh:
        _fh.write("[0 test]" + chr(10))
    with open(os.path.join(_prod_fake_env, "MonGen.txt"), "w", encoding="gbk") as _fh:
        _fh.write("")
    globals()["FIXTURE_PROD_ROOT"] = _prod_fake
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

    # G1（review 404c26b8）：**切换生效牙**。删掉 selftest 开头那句
    # `globals()["FIXTURE_PROD_ROOT"] = _prod_fake` ⇒ 本断言必红。
    # 注：它**故意**放在 `chk` 定义之后而不是函数最开头——最开头处 `chk` 还没定义。
    chk(os.path.realpath(FIXTURE_PROD_ROOT) != os.path.realpath(_prod_root_saved),
        "FX-FAKE-SWITCH 守卫牙必须以假根为靶（切换生效）")

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
        # settle 走 stub 的 mysql 分支：stub 自己记 "mysql <side>" 行（step 名不落日志），
        # 与 rest/stack/probe/db/stop 的断言无关 ⇒ 一并剔除。
        order_core = [x for x in order if not x.startswith("mysql ")]
        chk(order_core == expected,
            "编排顺序 = restore/stack/probe/save_settle*/db/stop x2（剔除 settle 轮询后实际 %s）"
            % order_core)
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
        disc_core = [x for x in disc_steps if x != "save_settle"]
        chk(disc_core == ["restore", "stack", "discover", "probe", "db", "stop"] * 2
            + ["compare"],
            "DISC-1 命令序列 discover 先于 probe（剔除 settle 轮询后实际 %s）" % disc_core)
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

        # 9) 生产库 => 默认拒绝（restore 用 stub 可执行位：裸 `bash` 自本卡起加载期即拒，
        #    不能再借它携带 mir2_db 文本——卡 openmir2-window-profile-env-hardening）
        prof_prod = _selftest_profile(tmp, stub, log)
        prof_prod["sides"]["csharp"]["restore"] = [stub, "-c", "mysql mir2_db < x"]
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

        def patched_runner_run(self, side, step, argv, env=None, timeout=600,
                               stdin_text=None, capture=False):
            if step == "stop" and stop_state["value"] != "orig":
                safe = _redact_argv(argv)
                self.commands.append({"side": side, "step": step, "argv": safe})
                self.steps.append("%s:%s" % (side, step))
                self.log("== %s/%s :: %s" % (side, step, _join_argv(safe)))
                self.log("   %s/%s rc=%s" % (side, step, stop_state["value"]))
                return (stop_state["value"], "", "") if capture else stop_state["value"]
            return orig_runner_run(self, side, step, argv, env=env, timeout=timeout,
                                   stdin_text=stdin_text, capture=capture)

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

        # 15d) 新内置 profile `window-magic`（卡 openmir2-window-profile-magic-preset；
        #      卡 openmir2-window-wmi-env-forwarding 收紧为「pin 只在 rust 侧 + 必须接线」）：
        #      两侧 magic 段（magic_id=1）+ rust 侧 D-9 pin=0 + rust stack 接线 -PinChecks/-PinLog，
        #      缺一 `spell_cast` 仍是「少前置/不可观测」状态。
        try:
            wm = load_profile("window-magic")
            wm_magic = profile_magic(wm)
            wm_side_magic = {s: profile_magic(wm, s) for s in wm["sides"]}
            wm_rs_env = (wm["sides"]["rust"].get("env") or {}).get(PIN_LAYOUT_KEY)
            wm_cs_env = (wm["sides"]["csharp"].get("env") or {}).get(PIN_LAYOUT_KEY)
            wm_wired = wmi_pin_checks(wm["sides"]["rust"].get("stack"))
            wm_ok = (bool(wm_magic) and len(wm_magic) == 1
                     and wm_magic[0]["magic_id"] == 1
                     and all(m and len(m) == 1 and m[0]["magic_id"] == 1
                             for m in wm_side_magic.values())
                     and wm_rs_env == "0"
                     and wm_cs_env is None
                     and wm_wired is not None
                     and ("%s=0" % PIN_LAYOUT_KEY) in wm_wired
                     and any("-PinLog" == str(x) for x in wm["sides"]["rust"]["stack"]))
            wm_err = ""
        except ConfigError as exc:
            wm_ok, wm_err = False, str(exc)
        chk(wm_ok, "15d) 内置 window-magic：两侧 magic 段(magic_id=1) + rust 侧 pin=0 + "
            "stack 接线 -PinChecks/-PinLog %s" % wm_err)

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
        # 20) BLOCKED 场景硬门（卡 openmir2-window-blocked-scenario-hard-gate）
        # 判据：合法 blocked ⇒ rc=2 且点名 reason/missing_fixture，**不建 out、不起栈、不写 report**；
        # 无 blocked ⇒ 一字不变地 rc=0（不误伤）。检查必须在下面的 rc 判定（fail_n）之前。
        def write_scen(doc, name):
            path = os.path.join(tmp, "scen-%s-%d.json" % (name, len(os.listdir(tmp))))
            with open(path, "w", encoding="utf-8", newline="\n") as f:
                json.dump(doc, f, ensure_ascii=False)
            return path

        b_reason = "夹具不存在：基线角色没有一档同时满足 DC_min>59 与 HitPoint>=17"
        b_fixture = "combat_strike 夹具矩阵（docs/combat-strike-scenario-20261023.md §6）"
        scen_b = write_scen({
            "name": "selftest_blocked",
            "blocked": {"reason": b_reason, "missing_fixture": b_fixture},
            "steps": [{"send": {"ident": 3030, "text": "@时间"}},
                      {"wait_for": 100}],
        }, "blocked")
        scen_unblocked = write_scen({
            "name": "selftest_unblocked",
            "steps": [{"send": {"ident": 3030, "text": "@时间"}},
                      {"wait_for": 100}],
        }, "unblocked")

        # 19a) --dry-run：rc=2、点名 reason 与 missing_fixture、不建 out、不预览任何命令
        fresh_log()
        out_b = os.path.join(tmp, "run-blocked-dry")
        s_b = execute(base_cfg(out_b, _selftest_profile(tmp, stub, log),
                               scenario=scen_b, dry_run=True))
        chk(s_b["exit_code"] == 2
            and any("scenario is BLOCKED: " + b_reason in n for n in s_b["notes"]),
            "BL-1 --dry-run 命中 blocked => rc=2 且打印 'scenario is BLOCKED: <reason>'")
        chk(any(b_fixture in n for n in s_b["notes"]),
            "BL-1 --dry-run 打印 missing_fixture")
        chk(s_b["verdict"] == "BLOCKED" and not s_b["commands"]
            and not os.path.exists(out_b) and not read_log().strip(),
            "BL-1 --dry-run 命中 blocked => 不建 out、不起栈、不预览命令（verdict=%s）"
            % s_b["verdict"])

        # 19b) 真跑：rc=2、不建 out 目录、不起栈、不写 report.json（与端口被占同等级）
        fresh_log()
        out_b2 = os.path.join(tmp, "run-blocked-real")
        s_b2 = execute(base_cfg(out_b2, _selftest_profile(tmp, stub, log), scenario=scen_b))
        chk(s_b2["exit_code"] == 2 and s_b2["verdict"] == "BLOCKED"
            and not os.path.exists(out_b2) and not read_log().strip(),
            "BL-2 真跑命中 blocked => rc=2、不建 out、不起栈、不写 report")
        chk(not os.path.exists(os.path.join(out_b2, "report.json")),
            "BL-2 真跑命中 blocked => 没有 report.json")

        # 19c) blocked 结构非法 ⇒ 加载期 ConfigError（fail-closed），一步都不执行
        for label, bad_bl in (("缺 reason", {"missing_fixture": "x"}),
                              ("blocked 非对象", "yes"),
                              ("未知键", {"reason": "x", "note": "y"}),
                              ("reason 空串", {"reason": "  "})):
            fresh_log()
            err_b = ""
            try:
                execute(base_cfg(os.path.join(tmp, "run-blocked-shape"),
                                 _selftest_profile(tmp, stub, log),
                                 scenario=write_scen({"name": "bad", "blocked": bad_bl,
                                                      "steps": [{"wait_for": 658}]},
                                                     "badshape"),
                                 dry_run=True))
            except ConfigError as e:
                err_b = str(e)
            chk("scenario.blocked" in err_b and not os.path.exists(log),
                "BL-3 blocked 形状非法（%s）=> ConfigError 且一步都没执行（%r）"
                % (label, err_b[:80]))

        # 19d) 不带 blocked 的场景不受影响（同形状、同样 dry-run => rc=0 且命令照常预览）
        fresh_log()
        s_bok = execute(base_cfg(os.path.join(tmp, "run-unblocked"),
                                 _selftest_profile(tmp, stub, log),
                                 scenario=scen_unblocked, dry_run=True))
        chk(s_bok["exit_code"] == 0 and s_bok["verdict"] == "DRY-RUN"
            and any(c["step"] == "stack" for c in s_bok["commands"]),
            "BL-4 无 blocked => 仍 rc=0 并照常预览（不误伤，实际 %s/%s）"
            % (s_bok["exit_code"], s_bok["verdict"]))

        # 20b) 夹具前置声明 `requires`（卡 `openmir2-scenario-fixture-precondition`）：
        #      场景显式声明「需要 magic 夹具」时，**加载期**按生效 profile 逐侧核段；
        #      缺段 => rc=2 并点名场景/profile/缺的侧（`--dry-run` 与真跑走同一条路径）。
        #      没有 requires 的场景在这里是 no-op（BL-4 就是对照：同 profile 同形状 => rc=0）。
        MAGIC_REQ_OK = [{"magic_id": 1, "level": 0, "use_key": "1", "curr_train": 0}]
        scen_req = write_scen({
            "name": "selftest_requires_magic",
            "requires": ["magic"],
            "steps": [{"send": {"ident": 3030, "text": "@时间"}},
                      {"wait_for": 100}],
        }, "requires")

        # RQ-1 声明 requires=["magic"] + profile 无 magic 段 => ConfigError（rc=2 口径）、
        #      消息点名场景 / profile / **两侧都缺**，且一步都没执行（不建 out、不起栈）。
        fresh_log()
        err_rq1 = ""
        try:
            execute(base_cfg(os.path.join(tmp, "run-req-missing"),
                             _selftest_profile(tmp, stub, log),
                             scenario=scen_req, dry_run=True))
        except ConfigError as e:
            err_rq1 = str(e)
        chk("场景 `selftest_requires_magic` 要求 `magic` 夹具" in err_rq1
            and "profile `selftest`" in err_rq1
            and "未声明 `magic` 段" in err_rq1
            and "缺 csharp、rust 侧" in err_rq1
            and not os.path.exists(log),
            "RQ-1 requires=['magic'] + profile 无 magic 段 => rc=2 并点名场景/profile/两侧（%r）"
            % err_rq1[:150])

        # RQ-2 同场景 + profile 有 magic 段 => rc=0，且**两侧都打印**「魔法夹具（dry-run 不连库）」
        #      预置行（把 dry-run 的假绿通道封死：有段才绿、绿了就有预置行）。
        #      这里必须接住 ConfigError：校验被写反（有段也判缺）时要变成**干净 FAIL**，
        #      不能让异常冒到 selftest 顶层变成 Traceback（牙 RQ2 的还原条件）。
        prof_rq2 = _selftest_profile(tmp, stub, log)
        prof_rq2["magic"] = MAGIC_REQ_OK
        fresh_log()
        rq2_err = io.StringIO()
        s_rq2 = None
        err_rq2 = ""
        try:
            with contextlib.redirect_stderr(rq2_err):
                s_rq2 = execute(base_cfg(os.path.join(tmp, "run-req-ok"), prof_rq2,
                                         scenario=scen_req, dry_run=True))
        except ConfigError as e:
            err_rq2 = str(e)
        rq2_lines = [l for l in rq2_err.getvalue().splitlines() if "魔法夹具" in l]
        chk(s_rq2 is not None and s_rq2["exit_code"] == 0
            and s_rq2["verdict"] == "DRY-RUN"
            and len(rq2_lines) == 2
            and any("db=mir2_test_db magic_ids=1" in l for l in rq2_lines)
            and any("db=mir2_test_db_r magic_ids=1" in l for l in rq2_lines),
            "RQ-2 同场景 + 有 magic 段 => rc=0 且两侧都打印魔法夹具预置行（%r%s）"
            % ([l.split("card_window: ")[-1] for l in rq2_lines],
               ("；意外 ConfigError=%r" % err_rq2[:120]) if err_rq2 else ""))

        # RQ-3 未知 requires 值（含未来才实现的值）=> 加载期 ConfigError 并点名，不让它静默不生效。
        fresh_log()
        err_rq3 = ""
        try:
            execute(base_cfg(os.path.join(tmp, "run-req-unknown"),
                             _selftest_profile(tmp, stub, log),
                             scenario=write_scen({
                                 "name": "selftest_requires_unknown",
                                 "requires": ["magic", "inventory"],
                                 "steps": [{"send": {"ident": 3030, "text": "@时间"}},
                                           {"wait_for": 100}],
                             }, "requnknown"),
                             dry_run=True))
        except ConfigError as e:
            err_rq3 = str(e)
        chk("scenario.requires 含未知夹具" in err_rq3
            and "inventory" in err_rq3 and not os.path.exists(log),
            "RQ-3 requires 含未知值 => rc=2 并点名（只认 magic；%r）" % err_rq3[:110])

        # RQ-4 两侧分别判：顶层无 magic、只有 sides.csharp.magic => 仍红且**只点名 rust**
        #      （一侧有一侧无也要红——不能因为 C# 侧够用就放过 Rust 侧）。
        prof_rq4 = _selftest_profile(tmp, stub, log)
        prof_rq4["sides"]["csharp"]["magic"] = MAGIC_REQ_OK
        fresh_log()
        err_rq4 = ""
        try:
            execute(base_cfg(os.path.join(tmp, "run-req-oneside"), prof_rq4,
                             scenario=scen_req, dry_run=True))
        except ConfigError as e:
            err_rq4 = str(e)
        chk("缺 rust 侧" in err_rq4 and "缺 csharp" not in err_rq4
            and not os.path.exists(log),
            "RQ-4 只有 csharp 侧有 magic 段 => rc=2 且只点名 rust 侧（%r）" % err_rq4[:120])

        # 21) 魔法夹具（卡 `openmir2-window-magic-fixture`）：profile 声明 => 每侧 restore 之后、
        #     起栈之前写 `characters_magic`；完成不了 rc=2 且不起栈。检查在 rc 判定之前。
        def magic_prof(magic, **kw):
            kw.setdefault("envir_roots", deploy)
            p = _selftest_profile(tmp, stub, log, **kw)
            p["magic"] = magic
            return p

        MAGIC_OK = [{"magic_id": 1, "level": 0, "use_key": "1", "curr_train": 0}]
        MAGIC_PRE = '{"1": {"level": 0, "use_key": "1", "curr_train": 0}}'

        def reset_magic_state():
            fresh_log()
            for s in SIDES:
                try:
                    os.remove(log + ".magic-" + s)
                except OSError:
                    pass

        # MG-1 已学且一致 => 不写库（action=noop），命令序列里没有 magic.write
        reset_magic_state()
        prof_m1 = magic_prof(MAGIC_OK)
        for s in SIDES:
            prof_m1["sides"][s]["env"] = {"STUB_MAGIC_INIT": MAGIC_PRE}
        out_m1 = os.path.join(tmp, "run-magic-noop")
        s_m1 = execute(base_cfg(out_m1, prof_m1, scenario=scen_unblocked))
        cs_m1 = (s_m1["sides"].get("csharp") or {}).get("magic") or {}
        rs_m1 = (s_m1["sides"].get("rust") or {}).get("magic") or {}
        chk(s_m1["exit_code"] == 0 and cs_m1.get("action") == "noop"
            and rs_m1.get("action") == "noop",
            "MG-1 已学且一致 => 两侧都不写库（csharp=%r rust=%r，rc=%s）"
            % (cs_m1.get("action"), rs_m1.get("action"), s_m1["exit_code"]))
        chk("magic.write" not in [c["step"] for c in s_m1["commands"]],
            "MG-1 已学且一致 => 命令序列里没有 magic.write（%s）"
            % [c["step"] for c in s_m1["commands"]][:10])

        # MG-2 未学 => 按声明 INSERT，且发生在 restore 之后、起栈之前；读回等于声明
        reset_magic_state()
        out_m2 = os.path.join(tmp, "run-magic-insert")
        s_m2 = execute(base_cfg(out_m2, magic_prof(MAGIC_OK), scenario=scen_unblocked))
        cs_m2 = (s_m2["sides"].get("csharp") or {}).get("magic") or {}
        steps_m2 = [(c["side"], c["step"]) for c in s_m2["commands"]
                    if c["side"] == "csharp"]
        chk(s_m2["exit_code"] == 0 and s_m2["verdict"] == "PASS"
            and cs_m2.get("action") == "written",
            "MG-2 未学 => 预置后 rc=0/PASS（action=%r，实际 %s/%s）"
            % (cs_m2.get("action"), s_m2["exit_code"], s_m2["verdict"]))
        names_m2 = [x[1] for x in steps_m2]
        chk("restore" in names_m2 and "magic.write" in names_m2
            and "stack" in names_m2
            and names_m2.index("restore") < names_m2.index("magic.write") < names_m2.index("stack"),
            "MG-2 预置发生在 restore 之后、起栈之前（%s）" % names_m2[:10])
        chk((cs_m2.get("after") or [{}])[0].get("found")
            == {"level": 0, "use_key": "1", "curr_train": 0}
            and cs_m2.get("player_id") == 6228,
            "MG-2 读回 = 声明（MagicId/Level/UseKey/CurrTrain），PlayerId 各侧自取（%s）"
            % ((cs_m2.get("after") or [{}])[0].get("found"),))
        try:
            with open(os.path.join(out_m2, "csharp", "magic.json"), encoding="utf-8") as f:
                mj = json.load(f)
            magic_json_ok = mj.get("action") == "written" and mj.get("sql", {}).get("write")
        except (OSError, ValueError):
            magic_json_ok = False
        chk(magic_json_ok, "MG-2 每侧落 magic.json（含 SQL 与前后值）")

        # MG-3 非法声明 => 加载期 ConfigError（rc=2 口径），一步都不执行
        fresh_log()
        err_m3 = ""
        try:
            execute(base_cfg(os.path.join(tmp, "run-magic-bad"),
                             magic_prof([{"magic_id": 0, "level": 0,
                                          "use_key": "1", "curr_train": 0}]),
                             scenario=scen_unblocked))
        except ConfigError as e:
            err_m3 = str(e)
        chk("profile.magic[0].magic_id" in err_m3 and not os.path.exists(log),
            "MG-3 非法声明（magic_id=0）=> ConfigError 且一步都没执行（%r）" % err_m3[:90])

        # MG-4 写后读回不符 => rc=2，且不起栈（不让 probe 去超时）
        reset_magic_state()
        prof_m4 = magic_prof(MAGIC_OK)
        for s in SIDES:
            prof_m4["sides"][s]["env"] = {"STUB_MAGIC_NOWRITE": s}
        out_m4 = os.path.join(tmp, "run-magic-readback")
        s_m4 = execute(base_cfg(out_m4, prof_m4, scenario=scen_unblocked))
        err_m4 = (s_m4["sides"].get("csharp") or {}).get("error") or ""
        chk(s_m4["exit_code"] == 2 and "读回不符" in err_m4,
            "MG-4 读回不符 => rc=2 并点名（%r）" % err_m4[:110])
        chk("stack" not in [c["step"] for c in s_m4["commands"]],
            "MG-4 预置失败 => 不起栈、不跑场景（%s）"
            % [c["step"] for c in s_m4["commands"]][:10])

        # MG-5 角色不存在 => rc=2 并点名
        reset_magic_state()
        prof_m5 = magic_prof(MAGIC_OK)
        for s in SIDES:
            prof_m5["sides"][s]["env"] = {"STUB_MAGIC_CHAR_COUNT": "0"}
        s_m5 = execute(base_cfg(os.path.join(tmp, "run-magic-nochar"), prof_m5,
                                scenario=scen_unblocked))
        err_m5 = (s_m5["sides"].get("csharp") or {}).get("error") or ""
        chk(s_m5["exit_code"] == 2 and "角色不存在" in err_m5,
            "MG-5 角色不存在 => rc=2 并点名（%r）" % err_m5[:110])

        # MG-6 --dry-run：只预览、不连库（没有 magic.character 等 DB 步骤）
        reset_magic_state()
        s_m6 = execute(base_cfg(os.path.join(tmp, "run-magic-dry"),
                                magic_prof(MAGIC_OK), scenario=scen_unblocked,
                                dry_run=True))
        mprev = ((s_m6.get("magic") or {}).get("sides") or {}).get("csharp") or {}
        chk(s_m6["exit_code"] == 0 and mprev.get("offline_check")
            and not [c for c in s_m6["commands"] if c["step"].startswith("magic.")],
            "MG-6 --dry-run 只预览魔法夹具、不连库（steps=%s）"
            % [c["step"] for c in s_m6["commands"]][:10])

        # MG-7 sides.<side>.magic 覆盖：顶层 magic=1 + sides.csharp.magic=2
        #     => csharp 声明/写入 MagicId=2，rust 保持顶层 MagicId=1。
        reset_magic_state()
        OVERRIDE = [{"magic_id": 2, "level": 1, "use_key": "2", "curr_train": 3}]
        prof_m7 = magic_prof(MAGIC_OK)
        prof_m7["sides"]["csharp"]["magic"] = OVERRIDE
        for s in SIDES:
            prof_m7["sides"][s]["env"] = {"STUB_MAGIC_INIT": MAGIC_PRE}
        out_m7 = os.path.join(tmp, "run-magic-override")
        s_m7 = execute(base_cfg(out_m7, prof_m7, scenario=scen_unblocked))
        cs_m7 = (s_m7["sides"].get("csharp") or {}).get("magic") or {}
        rs_m7 = (s_m7["sides"].get("rust") or {}).get("magic") or {}
        chk(s_m7["exit_code"] == 0
            and [m.get("magic_id") for m in (cs_m7.get("declared") or [])] == [2]
            and [m.get("magic_id") for m in (rs_m7.get("declared") or [])] == [1],
            "MG-7 sides.csharp.magic 覆盖顶层 => declared 两侧不同（csharp=%r rust=%r，rc=%s）"
            % ([m.get("magic_id") for m in (cs_m7.get("declared") or [])],
               [m.get("magic_id") for m in (rs_m7.get("declared") or [])],
               s_m7["exit_code"]))
        chk(cs_m7.get("action") == "written"
            and "VALUES (6228, 2," in ((cs_m7.get("sql") or {}).get("write") or "")
            and rs_m7.get("action") == "noop"
            and (rs_m7.get("sql") or {}).get("write") is None,
            "MG-7 写入按侧分派：csharp 写 MagicId=2、rust 不写（cs=%r rs=%r）"
            % (cs_m7.get("action"), rs_m7.get("action")))
        # 断言强度（卡 openmir2-magic-fixture-assertion-strength）：只钉 VALUES 抓不到
        # 「写错表/写错列」——必须钉住目标表与完整列清单（与 characters_magic DDL 一致）。
        write_stmt_m7 = ((cs_m7.get("sql") or {}).get("write") or "")
        chk(re.search(
                r"INSERT INTO `[^`]+`\.`characters_magic` \(`PlayerId`, `MagicId`, "
                r"`Level`, `UseKey`, `CurrTrain`\) VALUES \(6228, 2, 1, '2', 3\);",
                write_stmt_m7) is not None,
            "MG-7 写入语句钉到表名+列清单：INSERT INTO …`characters_magic`"
            "(PlayerId, MagicId, Level, UseKey, CurrTrain)（实际 %r）" % write_stmt_m7)
        cs_after_m7 = (cs_m7.get("after") or [{}])[0]
        rs_after_m7 = (rs_m7.get("after") or [{}])[0]
        chk(cs_after_m7.get("magic_id") == 2
            and (cs_after_m7.get("found") or {}).get("level") == 1
            and rs_after_m7.get("magic_id") == 1
            and (rs_after_m7.get("found") or {}).get("level") == 0,
            "MG-7 读回：csharp=MagicId2/level1、rust=MagicId1/level0（%r / %r）"
            % (cs_after_m7, rs_after_m7))
        chk("magic-columns csharp" in read_log() and "magic-columns rust" in read_log(),
            "MG-7 两侧 SHOW COLUMNS 都走 magic 桩（日志 magic-columns 标记）")
        # MG-7（dry-run）：摘要里的两侧 declared 必须按侧覆盖（顶层 1 / csharp 覆盖 2）
        s_m7d = execute(base_cfg(os.path.join(tmp, "run-magic-override-dry"), prof_m7,
                                 scenario=scen_unblocked, dry_run=True))
        mds = (s_m7d.get("magic") or {}).get("sides") or {}
        cs_d = [m.get("magic_id") for m in ((mds.get("csharp") or {}).get("declared") or [])]
        rs_d = [m.get("magic_id") for m in ((mds.get("rust") or {}).get("declared") or [])]
        chk(s_m7d["exit_code"] == 0 and cs_d == [2] and rs_d == [1],
            "MG-7 --dry-run 摘要两侧 declared 不同（csharp=%r rust=%r，rc=%s）"
            % (cs_d, rs_d, s_m7d["exit_code"]))

        # 18n) --var 透传（卡 openmir2-card-window-var-passthrough）：
        #   VAR-1 = 原样（token 逐字）出现在**两侧** probe 命令行、顺序 = 命令行顺序，
        #           且 --dry-run 打印出来的那一行里也能看到；
        #   VAR-2 = 不传时两侧 probe 命令行都不带 --var（默认行为不变）；
        #   VAR-3 = 四种非法形式在任何 IO 之前加载期 ConfigError 并点名。
        var_prof = _selftest_profile(tmp, stub, log)

        def _has_seq(argv, seq):
            n = len(seq)
            return any(list(argv[i:i + n]) == list(seq)
                       for i in range(len(argv) - n + 1))

        var_args = ["npc_actor=4242", "move_x=7"]
        want_var = ["--var", "npc_actor=4242", "--var", "move_x=7"]
        s_var = execute(base_cfg(os.path.join(tmp, "run-var-dry"), var_prof,
                                 scenario=scen_unblocked, dry_run=True,
                                 vars=var_args))
        var_probe = {c["side"]: c["argv"] for c in s_var["commands"]
                     if c["step"] == "probe"}
        chk(s_var["exit_code"] == 0
            and _has_seq(var_probe.get("csharp") or [], want_var),
            "VAR-1 --var 原样出现在 csharp probe 命令行（顺序一致，%s）"
            % (var_probe.get("csharp") or [])[-4:])
        chk(s_var["exit_code"] == 0
            and _has_seq(var_probe.get("rust") or [], want_var),
            "VAR-1 --var 原样出现在 rust probe 命令行（顺序一致，%s）"
            % (var_probe.get("rust") or [])[-4:])
        chk("--var npc_actor=4242 --var move_x=7"
            in _join_argv(var_probe.get("csharp") or []),
            "VAR-1 --dry-run 打印的行里可见 --var（%s）"
            % _join_argv(var_probe.get("csharp") or [])[-60:])
        s_novar = execute(base_cfg(os.path.join(tmp, "run-novar-dry"), var_prof,
                                   scenario=scen_unblocked, dry_run=True))
        novar_probe = [c["argv"] for c in s_novar["commands"] if c["step"] == "probe"]
        chk(s_novar["exit_code"] == 0 and len(novar_probe) == 2
            and all("--var" not in a for a in novar_probe),
            "VAR-2 不传 --var => 两侧 probe 命令行都不带 --var（默认不变）")
        bad_var_cases = [
            ("缺 '='", ["npc_actor119"]),
            ("空 key", ["=119"]),
            ("空 value", ["npc_actor="]),
            ("同名不同值", ["npc_actor=1", "npc_actor=2"]),
        ]
        for bad_label, bad_args in bad_var_cases:
            try:
                execute(base_cfg(os.path.join(tmp, "run-var-bad"), var_prof,
                                 scenario=scen_unblocked, dry_run=True,
                                 vars=bad_args))
                bad_err = None
            except ConfigError as e:
                bad_err = str(e)
            chk(bad_err is not None and "--var" in bad_err,
                "VAR-3 非法 --var（%s）=> 加载期 ConfigError 并点名（%r）"
                % (bad_label, bad_err))

        # 18m) 背包夹具（INV-1..）：4 用例 + 2 牙 + 生产接线/失败面——**必须走 chk**，
        # 也就是排在 rc 判定之前；此前这段放在 `if fail_n: return 1` 之后 ⇒ 牙永不判红（review `c866f358` B-1）。
        for cond, msg in _selftest_inventory(tmp):
            chk(cond, msg)
        for cond, msg in _selftest_inventory_reachability(tmp):
            chk(cond, msg)
        for cond, msg in _selftest_assets(tmp):
            chk(cond, msg)
        for cond, msg in _selftest_save_settle():
            chk(cond, msg)
        for cond, msg in _selftest_level_fixture(tmp):
            chk(cond, msg)
        for cond, msg in _selftest_postrun_symmetry(tmp):
            chk(cond, msg)
        for cond, msg in _selftest_inventory_fixture(tmp):
            chk(cond, msg)
        for cond, msg in _selftest_main_exit_code(tmp):
            chk(cond, msg)
        for cond, msg in _selftest_fixtures(tmp):
            chk(cond, msg)
        for cond, msg in _selftest_monsters_prepend(tmp):
            chk(cond, msg)
        for cond, msg in _selftest_fixtures_files(tmp, stub, log, base_cfg):
            chk(cond, msg)
        for cond, msg in _selftest_rng_ledger(tmp):
            chk(cond, msg)

        # ⑰ execute 读 res["inventory"]（卡 openmir2-card-window-teeth-mg7）：
        #    背包夹具失败 => summary 记该侧证据、整体 rc=2、且不起栈。
        inv_scen = write_scen({
            "name": "selftest_inventory_exec",
            "inventory": [{"index": 100, "make_index": 5001, "position": 3,
                           "std_mode": 0}],
            "steps": [{"send": {"ident": 3030, "text": "@时间"}}, {"wait_for": 100}],
        }, "invexec")
        fresh_log()
        prof_inv = _selftest_profile(tmp, stub, log, envir_roots=deploy)
        s_inv = execute(base_cfg(os.path.join(tmp, "run-inventory-exec"), prof_inv,
                                 scenario=inv_scen))
        cs_inv = (s_inv["sides"].get("csharp") or {})
        inv_side = ((s_inv.get("inventory") or {}).get("sides") or {}).get("csharp") or {}
        chk(s_inv["exit_code"] == 2
            and (cs_inv.get("inventory") or {}).get("rc") == 2
            and inv_side.get("rc") == 2
            and "stack" not in [c["step"] for c in s_inv["commands"]],
            "⑰ execute 读 res['inventory']：失败 => summary 记证据、rc=2、不起栈（%r）"
            % (((cs_inv.get("inventory") or {}).get("error") or "")[:80],))

        # 18n) 资产声明通道（卡 openmir2-window-gold-declaration-channel）：
        #     dry-run 正牙：assets 预览 rc=0 且 UPDATE 命令预览行出现 SET Gold=10000；
        #     真跑失败路径：run_assets rc=2 => summary 记证据、整体 rc=2、不起栈（同 ⑰）。
        scen_ast = write_scen({
            "name": "selftest_assets",
            "assets": {"Gold": 10000, "note": "upgrade_retrieve 前置"},
            "steps": [{"send": {"ident": 3030, "text": "@时间"}}, {"wait_for": 100}],
        }, "assets")
        fresh_log()
        prof_ast = _selftest_profile(tmp, stub, log, envir_roots=deploy)
        s_ast_dry = execute(base_cfg(os.path.join(tmp, "run-assets-dry"), prof_ast,
                                     scenario=scen_ast, dry_run=True))
        ast_dry_cs = ((s_ast_dry.get("assets") or {}).get("sides") or {}).get("csharp") or {}
        ast_stdins = [c.get("stdin") or "" for c in ast_dry_cs.get("commands") or []]
        chk(s_ast_dry["exit_code"] == 0 and ast_dry_cs.get("rc") == 0
            and any("SET Gold=10000" in s for s in ast_stdins),
            "18n dry-run：assets 预览 rc=0 且命令预览行出现 SET Gold=10000（%r）"
            % ((ast_dry_cs.get("error") or "")[:80],))

        orig_run_assets = globals()["run_assets"]
        globals()["run_assets"] = lambda *a, **k: {
            "rc": 2, "error": "资产声明通道失败：assets.update 影响了 0 行（期望 1）"}
        fresh_log()
        try:
            prof_ast2 = _selftest_profile(tmp, stub, log, envir_roots=deploy)
            s_ast = execute(base_cfg(os.path.join(tmp, "run-assets-exec"), prof_ast2,
                                     scenario=scen_ast))
        finally:
            globals()["run_assets"] = orig_run_assets
        cs_ast = (s_ast["sides"].get("csharp") or {})
        ast_side = ((s_ast.get("assets") or {}).get("sides") or {}).get("csharp") or {}
        chk(s_ast["exit_code"] == 2
            and (cs_ast.get("assets") or {}).get("rc") == 2
            and ast_side.get("rc") == 2
            and "stack" not in [c["step"] for c in s_ast["commands"]],
            "18n execute 读 res['assets']：失败 => summary 记证据、rc=2、不起栈（%r）"
            % (((cs_ast.get("assets") or {}).get("error") or "")[:80],))

        # 22) 内置 profile `window-testdb`（卡 openmir2-window-profile-testdb-preset）：
        #     alt 的副本 + C# 侧显式指向测试库 ⇒ 5 个带预置位的场景不再命中生产库守卫。
        #     断言的是「输入不命中守卫」，**不是**「守卫放过」：守卫代码一字未动。
        try:
            wtd = load_profile("window-testdb")
            wtd_cs = (wtd["sides"]["csharp"].get("db") or {})
            wtd_rs = (wtd["sides"]["rust"].get("db") or {})
            wtd_ok = (wtd_cs.get("preset") == "test"
                      and wtd_cs.get("characters_db") == "mir2_test_db"
                      and classify_db_name(wtd_cs.get("characters_db")) == "test"
                      and classify_db_name(wtd_rs.get("characters_db")) == "test")
            # 非 dry-run、不容许生产库：若仍命中守卫会抛 ConfigError ⇒ 断言失败。
            check_profile_safety(wtd, False, dry_run=False)
            wtd_err = ""
        except ConfigError as exc:
            wtd_ok, wtd_err = False, str(exc)
        chk(wtd_ok, "22) 内置 window-testdb：C# 侧 test 库(mir2_test_db) + Rust 侧测试库，"
            "且不触发生产库守卫 %s" % wtd_err)

        # 22b) 钉子：钉住 `_derived_testdb_profile` 里那两行**显式赋值**（返工项，reviewer-5 变异 B）。
        #     为什么不能只比值：`alt` 的 `sides.csharp.db` 本来就是
        #     {preset: test, characters_db: mir2_test_db}，所以「删掉两行、退化成纯别名」
        #     在最终 dict 上与现在**逐键相同** —— 只断言 load_profile("window-testdb") 的值，
        #     把赋值删了也照样 GREEN。牙必须打在构造器的**行为**上。
        #     做法：给构造器一个 C# 侧指向生产库的合成基线，输出仍必须被那两行强制改写成
        #     test / mir2_test_db（删掉任一行 ⇒ 输出保留该键的生产库值 ⇒ 本断言红）；
        #     同时基线本身不得被改动（deepcopy），且内置条目确实是该构造器作用于 `alt` 的产物。
        synth_base = {"name": "synth-base", "description": "",
                      "sides": {"csharp": {"db": {"preset": "prod",
                                                  "characters_db": "mir2_db"}},
                                "rust": {"db": {"characters_db": "mir2_test_db_r"}}}}
        synth_out = _derived_testdb_profile(synth_base, "synth-out", "selftest")
        synth_db = synth_out["sides"]["csharp"]["db"]
        wtd_builtin_db = (BUILTIN_PROFILES["window-testdb"]["sides"]["csharp"].get("db") or {})
        # carried #119：第 4 合取项原为 `wtd_builtin_db == from_alt_db`（恒真——
        # alt 的 C# db 本就是 test/mir2_test_db）。改为钉「名字是构造器所赋」：
        # 纯别名 deepcopy(alt) 的 name 仍是 "alt" ⇒ 本断言红。
        wtd_builtin_name = BUILTIN_PROFILES["window-testdb"].get("name")
        chk(synth_db.get("preset") == "test"
            and synth_db.get("characters_db") == "mir2_test_db"
            and synth_base["sides"]["csharp"]["db"]
            == {"preset": "prod", "characters_db": "mir2_db"}
            and wtd_builtin_name == "window-testdb",
            "22b) `_derived_testdb_profile` 的两行显式赋值仍在且取值正确："
            "生产库基线也被强制改写成 test / mir2_test_db、基线不被改、"
            "内置条目名确为构造器所赋（实际 %r / name=%r）" % (synth_db, wtd_builtin_name))

        # 23) 环境加固（卡 openmir2-window-profile-env-hardening，window-run-1 §1 三处阻塞）：
        #     23a 裸 `bash` / System32 WSL stub 的 restore 加载期拒绝；
        #     23b 非 stub 的 Git bash 绝对路径放行；
        #     23c 内置 alt/csharp-default：restore 非裸 bash、stack 走 WMI 包装、
        #         ready_ports 不含网关端口且包装 -ReadyPorts 兜住网关。
        for bad_label, bad_restore in (
                ("裸 bash", ["bash", "E:/tmp/conformance/restore-baseline.sh"]),
                ("System32 stub", ["C:/Windows/System32/bash.exe",
                                   "E:/tmp/conformance/restore-baseline.sh"])):
            p_bash = _selftest_profile(tmp, stub, log)
            p_bash["sides"]["csharp"]["restore"] = bad_restore
            bash_err = ""
            try:
                load_profile(p_bash)
            except ConfigError as e:
                bash_err = str(e)
            chk("bash" in bash_err,
                "23a restore %s => ConfigError 点名 bash/WSL stub（%r）"
                % (bad_label, bash_err[:90]))
        p_gitbash = _selftest_profile(tmp, stub, log)
        p_gitbash["sides"]["csharp"]["restore"] = [
            "C:/Program Files/Git/usr/bin/bash.exe",
            "E:/tmp/conformance/restore-baseline.sh"]
        gitbash_ok = True
        try:
            load_profile(p_gitbash)
        except ConfigError:
            gitbash_ok = False
        chk(gitbash_ok, "23b restore Git bash 绝对路径 => 加载通过")
        for pname, side_gws in (("alt", {"csharp": (17200,), "rust": (17600,)}),
                                ("csharp-default", {"csharp": (7200,), "rust": (7200,)})):
            prof_gw = BUILTIN_PROFILES[pname]
            for side in SIDES:
                sc_gw = prof_gw["sides"][side]
                gateways = side_gws[side]
                stack_gw = sc_gw["stack"]
                rest_gw = str(sc_gw["restore"][0])
                chk(not (os.path.basename(rest_gw).lower() in ("bash", "bash.exe")
                         and not os.path.dirname(rest_gw))
                    and not is_wsl_stub_path(rest_gw),
                    "23c %s.%s restore 用 Git bash 绝对路径（实际 %r）"
                    % (pname, side, rest_gw))
                chk(any("start-stack-wmi.ps1" in str(a) for a in stack_gw),
                    "23c %s.%s stack 走 WMI 包装（断开 stdout 管道继承）"
                    % (pname, side))
                rp_at = (stack_gw.index("-ReadyPorts")
                         if "-ReadyPorts" in stack_gw else None)
                wrapper_ports = ([int(p) for p in str(stack_gw[rp_at + 1]).split(",")]
                                 if rp_at is not None else [])
                gw_missing = [p for p in gateways
                              if p in (sc_gw.get("ready_ports") or [])]
                gw_uncovered = [p for p in gateways if p not in wrapper_ports]
                chk(not gw_missing and not gw_uncovered,
                    "23c %s.%s 网关 %r 不在 ready_ports（残留 %r）"
                    "且包装 -ReadyPorts 兜住（缺 %r）"
                    % (pname, side, gateways, gw_missing, gw_uncovered))

        # 24) WMI 派生链的 env 转发 + pin 自证（卡 openmir2-window-wmi-env-forwarding）。
        #     24a 静态：wmi_stack_argv 带 pin 参数 ⇒ -PinChecks/-PinLog 按序追加；
        #     24b-g 行为（真 powershell，-CheckPinsOnly 离线跑：不派生/不碰端口/WMI）：
        #       一致绿 ×2（oracle-40 / client-58 两分支）+ 不一致红 ×3（<unset> 日志、
        #       调用方缺 pin、调用方值与期望不一致——53 分诊实锤的三种形态都 fail-closed）；
        #     24h 静态：加载期守卫——声明 pin 的 WMI stack 不接线 ⇒ ConfigError。
        pin_log_good = os.path.join(tmp, "pin-good.log")      # pin 到达且生效（oracle-40）
        pin_log_bad = os.path.join(tmp, "pin-bad.log")        # pin 丢失（53 分诊实锤形态）
        pin_log_c58 = os.path.join(tmp, "pin-client58.log")   # 未 pin 的默认（client-58）
        with open(pin_log_good, "w", encoding="utf-8", newline="\n") as f:
            f.write("05:57:06.620 [INF] CLIENT_ABILITY_LAYOUT layout=oracle-40"
                    "（MIR2_CLIENT_ABILITY_LAYOUT=0）\n")
        with open(pin_log_bad, "w", encoding="utf-8", newline="\n") as f:
            f.write("05:57:06.620 [INF] CLIENT_ABILITY_LAYOUT layout=client-58"
                    "（MIR2_CLIENT_ABILITY_LAYOUT=<unset>）\n")
        with open(pin_log_c58, "w", encoding="utf-8", newline="\n") as f:
            f.write("05:57:06.620 [INF] CLIENT_ABILITY_LAYOUT layout=client-58"
                    "（MIR2_CLIENT_ABILITY_LAYOUT=1）\n")
        argv_24a = wmi_stack_argv("E:/tmp/conformance/start-rust.ps1", (15000, 17000),
                                  pin_checks=["MIR2_CLIENT_ABILITY_LAYOUT=0"],
                                  pin_log=RUST_STACK_PIN_LOG)
        chk(argv_24a[-4:] == ["-PinChecks", "MIR2_CLIENT_ABILITY_LAYOUT=0",
                              "-PinLog", RUST_STACK_PIN_LOG],
            "24a wmi_stack_argv 带 pin 参数 ⇒ -PinChecks/-PinLog 按序追加（尾 4 项 %r）"
            % (argv_24a[-4:],))

        def run_pin_wrapper(pin_log, env_val, expect):
            env = os.environ.copy()
            if env_val is None:
                env.pop(PIN_LAYOUT_KEY, None)
            else:
                env[PIN_LAYOUT_KEY] = env_val
            argv = POWERSHELL + ["-File", WMI_STACK_WRAPPER, "-CheckPinsOnly",
                                 "-SkipPortWait", "-PinChecks",
                                 "%s=%s" % (PIN_LAYOUT_KEY, expect),
                                 "-PinLog", pin_log, "-PinWaitSec", "1"]
            proc = subprocess.run(argv, capture_output=True, text=True, env=env,
                                  timeout=120)
            return proc.returncode, ((proc.stdout or "") + (proc.stderr or ""))

        rc, outp = run_pin_wrapper(pin_log_good, "0", "0")
        chk(rc == 0, "24b 行为绿：调用方 pin=0 + 日志 oracle-40（=0）⇒ rc=0（%r）" % outp[-80:])
        rc, outp = run_pin_wrapper(pin_log_bad, "0", "0")
        chk(rc != 0 and "<unset>" in outp,
            "24c 行为红：日志还是 <unset>（53 分诊实锤形态）⇒ rc≠0 并点名观测行（%r）"
            % outp[-90:])
        rc, outp = run_pin_wrapper(pin_log_good, None, "0")
        chk(rc != 0 and PIN_LAYOUT_KEY in outp and "不在调用方环境" in outp,
            "24d 行为红：调用方 env 没有该 pin ⇒ 派生之前就 rc≠0（%r）" % outp[-90:])
        rc, outp = run_pin_wrapper(pin_log_good, "1", "0")
        chk(rc != 0 and "不一致" in outp,
            "24e 行为红：调用方 pin=1 但期望 0 ⇒ env 与 -PinChecks 不一致 rc≠0（%r）"
            % outp[-90:])
        rc, outp = run_pin_wrapper(pin_log_c58, "1", "1")
        chk(rc == 0, "24f 行为绿：未 pin 的默认布局（client-58，=1）⇒ rc=0（%r）" % outp[-80:])
        p_pin = _selftest_profile(tmp, stub, log)
        p_pin["sides"]["rust"]["stack"] = wmi_stack_argv("E:/tmp/x/inner.ps1", (15000,))
        p_pin["sides"]["rust"]["env"] = {PIN_LAYOUT_KEY: "0"}
        pin_guard_err = ""
        try:
            load_profile(p_pin)
        except ConfigError as e:
            pin_guard_err = str(e)
        chk("-PinChecks" in pin_guard_err,
            "24h 加载期守卫：声明 pin 的 WMI stack 不接线 ⇒ ConfigError 并点名 -PinChecks（%r）"
            % pin_guard_err[:100])

    finally:
        holder.close()
        # C3：还原模块全局（否则后续 selftest/真实路径会读到假根——那又是一个新洞）。
        globals()["FIXTURE_PROD_ROOT"] = _prod_root_saved
        # 还原本身要有牙：写错（还原成别的假值/只还原一半）会让后续用例读到假根，
        # 同样以「selftest 全绿」的形式存在。
        chk(globals()["FIXTURE_PROD_ROOT"] == _prod_root_saved,
            "FX-PROD-RESTORE selftest 结束必须还原真实生产根")
        import shutil
        shutil.rmtree(tmp, ignore_errors=True)

    # H1（merge_result d9a0dd2fe）：生产 sha 牙必须在 `if fail_n: return 1` **之前**判定，
    # 否则一有其它失败就永远跑不到它（同 review c866f358 B-1 的形态）。
    if _prod_mon_sha0 is not None:
        _sha_now = hashlib.sha256(open(_prod_mon_real, "rb").read()).hexdigest()
        chk(_sha_now == _prod_mon_sha0,
            "FX-MON 真实生产 MonGen.txt sha256 全程未变（%s）" % _prod_mon_sha0[:12])
        if _prod_conf_sha0 is not None:
            _cnow = hashlib.sha256(open(_prod_conf_real, "rb").read()).hexdigest()
            chk(_cnow == _prod_conf_sha0,
                "FX-CONF 真实生产 Server.conf sha256 全程未变（%s）"
                % _prod_conf_sha0[:12])
    else:
        # G2 之后这条**可达**（假根不再依赖真实部署）：语义是「无部署环境 ⇒ 本牙不适用」，
        # 不是「跳过不查」——没有靶就没有可查对象，且 `_prod_mon_sha0 is None` 本身可判。
        chk(True, "FX-MON 无部署环境（无真实生产根）⇒ 不适用（有部署时按 sha 比对）")

    # J1：total 必须在 H1 块（FX-MON/FX-CONF）之后计算，否则计数自相矛盾。
    total = pass_n + fail_n
    if fail_n:
        print("RED: card_window.py selftest %d/%d" % (pass_n, total))
        return 1

    # 卡 `openmir2-gm-script-fixture` A5：`fixtures.files`（脚本注入通道）
    CRLF = bytes((13, 10))
    NL = chr(13) + chr(10)
    fr = tempfile.mkdtemp(prefix="cw_files_")
    froot = os.path.join(fr, "M2GameSvr")
    os.makedirs(os.path.join(froot, "Envir", "Market_Def", "shabake"), exist_ok=True)
    existing = os.path.join(froot, "Envir", "Market_Def", "shabake", "shop-0153.txt")
    OLD = b"(@buy @sell)" + CRLF + b"%100" + CRLF + b"+0" + CRLF
    with open(existing, "wb") as fh:
        fh.write(OLD)
    fsc = {"envir_root": froot}
    fside = os.path.join(fr, "side")
    os.makedirs(fside, exist_ok=True)

    def _fv(decl):
        try:
            return _validate_fixture_decl(decl, "t"), None
        except ConfigError as exc:
            return None, str(exc)

    okA, _errA = _fv({"files": {"Envir/Market_Def/shabake/shop-0153.txt": "new" + NL}})
    chk(okA is not None, "FX-F1 files 合法声明 => 通过")
    for bad, tag in (
        ({"files": {"Envir/../x.txt": "a"}}, "FX-F2 禁 `..`"),
        ({"files": {"/abs.txt": "a"}}, "FX-F3 禁绝对路径"),
        ({"files": {"Castle/List.txt": "a"}}, "FX-F4 只许 Envir/ 下"),
        ({"files": {"Envir/x.txt": ""}}, "FX-F5 文本非空"),
    ):
        got, berr = _fv(bad)
        chk(got is None and bool(berr), tag + " => ConfigError")

    fdecl = _validate_fixture_decl({"files": {
        "Envir/Market_Def/shabake/shop-0153.txt": "changed" + NL,
        "Envir/QuestDiary/new_script.txt": "GMEXECUTE @Level0 Self 5" + NL,
    }}, "t")
    fres = run_fixtures(Runner(fr, None, dry_run=False), "rust", {}, fsc, fdecl, {}, fside)
    newp = os.path.join(froot, "Envir", "QuestDiary", "new_script.txt")
    chk(fres.get("rc") == 0 and os.path.isfile(newp), "FX-F6 files 可新建脚本文件")
    recs = {r["rel"]: r for r in fres.get("files") or []}
    chk(recs["Envir/Market_Def/shabake/shop-0153.txt"]["before_sha256"]
        == hashlib.sha256(OLD).hexdigest(), "FX-F7 替换型记录写前 sha256")
    chk(recs["Envir/QuestDiary/new_script.txt"].get("created") is True,
        "FX-F8 新建型记录 created=True")

    root2 = os.path.join(fr, "M2GameSvr2")
    os.makedirs(os.path.join(root2, "Envir", "Market_Def", "shabake"), exist_ok=True)
    with open(os.path.join(root2, "Envir", "Market_Def", "shabake", "shop-0153.txt"),
              "wb") as fh:
        fh.write(OLD)
    side2 = os.path.join(fr, "side2")
    os.makedirs(side2, exist_ok=True)
    fres2 = run_fixtures(Runner(fr, None, dry_run=False), "csharp",
                         {}, {"envir_root": root2}, fdecl, {}, side2)
    sha_a = [r["after_sha256"] for r in fres["files"]
             if r["rel"].endswith("shop-0153.txt")][0]
    sha_b = [r["after_sha256"] for r in fres2["files"]
             if r["rel"].endswith("shop-0153.txt")][0]
    chk(sha_a == sha_b, "FX-F9 两侧写后 sha256 相同（逐字节相同）")

    ferr = restore_fixtures(fres)
    chk(ferr is None, "FX-F10 还原无错")
    with open(existing, "rb") as fh:
        cur = fh.read()
    chk(hashlib.sha256(cur).hexdigest()
        == recs["Envir/Market_Def/shabake/shop-0153.txt"]["before_sha256"],
        "FX-F11 替换型还原后 sha256 == 写前")
    chk(not os.path.exists(newp), "FX-F12 新建型还原后文件不存在")

    # 生产根守卫（A4/内容冻结）：① envir_root 直指生产；② Envir 为指向生产的符号链接；
    # ③ monsters 的 MonGen.txt 目标指向生产。**C3（review 62ac57e1）**：这三条牙一律把模块
    # 全局 `FIXTURE_PROD_ROOT` 临时改成**假根**——守卫坏掉时变异也只写 temp，绝不可能写真实
    # 生产（「一个测试的安全性不能依赖被测代码是正确的」）。
    _saved_prod = globals()["FIXTURE_PROD_ROOT"]
    _fake_prod = os.path.join(fr, "fake-prod")
    _fake_env = os.path.join(_fake_prod, "M2GameSvr", "Envir")
    os.makedirs(_fake_env, exist_ok=True)
    with open(os.path.join(_fake_env, "MapInfo.txt"), "w", encoding="gbk") as _fh:
        _fh.write("[0 test]" + chr(10))
    with open(os.path.join(_fake_env, "MonGen.txt"), "w", encoding="gbk") as _fh:
        _fh.write("")
    globals()["FIXTURE_PROD_ROOT"] = _fake_prod
    chk(os.path.realpath(FIXTURE_PROD_ROOT) != os.path.realpath(_saved_prod),
        "FX-FAKE-SWITCH(局部) 守卫牙必须以假根为靶（切换生效）")
    try:
        prod_res = run_fixtures(Runner(fr, None, dry_run=False), "rust", {},
                                {"envir_root": _fake_prod + "/M2GameSvr"},
                                _validate_fixture_decl(
                                    {"files": {"Envir/x.txt": "a"}}, "t"), {}, fside)
        chk(prod_res.get("rc") == 2 and "生产内容根" in (prod_res.get("error") or ""),
            "FX-F13 envir_root 指向生产根 => rc=2 拒绝")

        link_root = os.path.join(fr, "M2GameSvr-link")
        os.makedirs(link_root, exist_ok=True)
        try:
            os.symlink(_fake_prod + "/M2GameSvr/Envir",
                       os.path.join(link_root, "Envir"), target_is_directory=True)
            link_res = run_fixtures(Runner(fr, None, dry_run=False), "rust", {},
                                    {"envir_root": link_root},
                                    _validate_fixture_decl(
                                        {"files": {"Envir/x.txt": "a"}}, "t"), {}, fside)
            chk(link_res.get("rc") == 2
                and "解析到生产内容根" in (link_res.get("error") or ""),
                "FX-F14 Envir 为指向生产的符号链接 => 逐文件 realpath 守卫 rc=2")
        except OSError:
            chk(True, "FX-F14 跳过（本机无法建符号链接）")

        mroot = os.path.join(fr, "M2GameSvr-mongen")
        os.makedirs(mroot, exist_ok=True)
        try:
            os.symlink(_fake_prod + "/M2GameSvr/Envir",
                       os.path.join(mroot, "Envir"), target_is_directory=True)
            mon_decl = _validate_fixture_decl(
                {"monsters": [{"map": "0", "x": 1, "y": 1, "name": "x"}]}, "t")
            mres = run_fixtures(Runner(fr, None, dry_run=False), "rust", {},
                                {"envir_root": mroot}, mon_decl, {}, fside)
            chk(mres.get("rc") == 2 and "生产内容根" in (mres.get("error") or ""),
                "FX-F17 monsters 目标指向生产 => 逐目标守卫 rc=2")
        except OSError:
            chk(True, "FX-F17 跳过（本机无法建符号链接）")
    finally:
        globals()["FIXTURE_PROD_ROOT"] = _saved_prod

    _prof_path = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                              "..", "window", "profiles", "gm_script_level0.json")
    try:
        with open(_prof_path, encoding="utf-8") as fh:
            _p = json.load(fh)
        load_profile(_p)
        chk(True, "FX-F15 load_profile 能加载声明 files 的 profile（C1）")
    except Exception as exc:  # noqa: BLE001
        chk(False, "FX-F15 load_profile 能加载声明 files 的 profile（C1）：%r" % (exc,))
    try:
        json.dumps(fres)
        chk(True, "FX-F16 含 files 的 summary 可 json 序列化（C2）")
    except TypeError as exc:
        chk(False, "FX-F16 含 files 的 summary 可 json 序列化（C2）：%r" % (exc,))


    print("GREEN: card_window.py selftest %d/%d" % (pass_n, pass_n + fail_n))
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
    ap.add_argument("--scenario",
                    help="场景 JSON（tools/probe/scenarios/*.json）。场景顶层 `blocked`"
                         "（{reason, [missing_fixture]}）=> 本工具 rc=2 且不建 out/不起栈/不写 report："
                         "封存场景不得被窗口批次消费")
    ap.add_argument("--out", help="产物目录（真跑必填；--dry-run 可省）")
    ap.add_argument("--profile", default="alt",
                    help="profile：%s | <profile.json 路径>（默认 alt）"
                         % " | ".join(sorted(BUILTIN_PROFILES)))
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
    ap.add_argument("--var", action="append", default=[], metavar="NAME=VALUE",
                    help="透传给两侧 probe 的场景变量覆值（可重复，如 --var npc_actor=119）；"
                         "加载期校验：缺 '=' / 空名 / 空值 / 同名不同值 => rc=2 点名")
    ap.add_argument("--selftest", action="store_true", help="离线自检（桩命令 + 合成 frames）")
    ap.add_argument("--allow-production-db", action="store_true",
                    help="显式放行生产库配置（仅 owner 授权的窗口；默认拒绝）")
    ap.add_argument("--rules", default=DEFAULT_RULES, help="compare_runs.py 规则表")
    ap.add_argument("--login-gate", help="覆盖 profile 的 LoginGate HOST:PORT")
    ap.add_argument("--account", help="覆盖 profile 的账号（也可用 CONFORMANCE_ACCOUNT）")
    ap.add_argument("--server-name", help="覆盖 profile 的服务器名")
    ap.add_argument("--char-name", help="覆盖 profile 的角色名")
    ap.add_argument("--timeout-ms", type=int, help="覆盖 profile 的 probe --timeout-ms")
    ap.add_argument("--rng-log", action="store_true",
                    help="每侧把 RNG 流水落到 <out>/<side>/rng-stream.tsv 并校验"
                         "（INJECT_OK/SEED_PROBE + site 齐）；缺账本 rc=2")
    return ap


def main_exit_code(summary):
    """真实进程退出码（卡 `openmir2-runbatch-rc-attribution`）。

    **`0` 是合法值**：历史实现 `int(summary.get("exit_code") or 2)` 在 PASS（0）时
    因 `0 or 2` 求值为 2，导致**每个非 DRY-RUN 运行都以 rc=2 退出**（实测见
    docs/runbatch-rc-attribution-20261025.md）。这里只在 **None** 时回落到 2。
    """
    code = summary.get("exit_code")
    # `None` 与 `bool` 都视为「没有合法退出码」：`False` 若被 int() 会变成 0（假绿），
    # `True` 会变成 1（非法含义）——两者都 fail-closed 到 2（review `ee0122a8` F1）。
    if code is None or isinstance(code, bool):
        return 2
    return int(code)


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
        "vars": args.var,
        "allow_production_db": args.allow_production_db,
        "rules": args.rules,
        "login_gate": args.login_gate,
        "account": args.account,
        "server_name": args.server_name,
        "char_name": args.char_name,
        "timeout_ms": args.timeout_ms,
        "rng_log": args.rng_log,
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
    return main_exit_code(summary)


if __name__ == "__main__":
    sys.exit(main())
