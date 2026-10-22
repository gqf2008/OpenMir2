#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""备用端口剖面（15xxx / 17xxx）：生成六份影子 conf、可选整套部署根与客户端启动命令。

为什么要有这个工具
------------------
真机窗口现在必须占用 5000 / 7000 / 7200（以及 5100 / 5500 / 5600 / 6000 / 7100），
与外部 C# 全栈直接冲突。本工具把整套链路的端口**整体平移到备用段**。

两种模式（都只读源文件，绝不改 `E:\\MirServer` 与 `run-release.cmd`）：

1. **覆盖层（overlay）**：`generate --out-dir DIR` 只写六份影子 conf + 清单。
   `DIR` 不是可独立起栈的 `-Root`，必须与完整角色部署合并（见 runbook §8.3/§8.7）。
2. **部署根（deploy）**：`generate --deploy DIR --mir-server-root E:/MirServer`
   先把六个角色树从 `E:\\MirServer` **只读复制**到 `DIR`，再写六份影子 conf，
   并改写路由文件（`LoginSrv/{AddrTable,ServerAddr,UserLimit}.txt`、
   `DBSrv/{AddrTable,ServerInfo}.txt`），使客户端被路由到 17100（SelGate）与
   17200（GameGate）。产出可直接作为 `start_mir2_server.ps1 -Root DIR`。

固定剖面
--------
    game-svr   15000                       GameSvr 监听（GameGate 下游）
    db-srv     16000 / 15100 / 15700       DataService / SelGate 面 / Market
    login-srv  15500 / 15600 / 16005       LoginGate 面 / IDS / RunPort
    login-gate 17000                       客户端登录入口
    sel-gate   17100                       客户端选服入口
    game-gate  17200 / 17201 / 17202       客户端游戏入口（保留 Count=3 的间距）

键名/语义以仓库内 Rust 解析器为准（不是猜的）：
  - `bins/db-srv/src/config.rs`：[Setup] ServerPort=DataService(6000)、GatePort=SelGate 面(5100)；
    [Server] MarketPort=5700、IDSPort=LoginSrv IDS(5600)。
  - `bins/login-srv/src/config.rs`：[Server] GatePort=LoginGate 面(5500)、ServerPort=IDS(5600)；
    `DBSPort` 指向 db-srv DataService（样板 6000）；`RunPort` 是遗留键，样板 7200，统一切到 16005
    （与 SelGate 的 `LsvServerPort=16005` 对齐）。
    **`DBSPort` / `RunPort` 都是 legacy/未消费**：C# `ConfigManager` 只把 `DBSPort` 读进
    `Config.nDBSPort`（全仓无消费者），`RunPort` 连 C# 也不解析；Rust `login-srv` 两者都不读。
    保留它们只为与现网配置同形。
  - `bins/game-svr/src/config.rs`：[Server] GatePort=监听(5000)、DBPort=db-srv DataService(6000)、
    IDSPort=login-srv IDS(5600)。
  - `bins/login-gate/src/config.rs`：[LoginGate] ServerPort0=LoginSrv(5500)、GatePort0=客户端(7000)。
  - `bins/sel-gate/src/config.rs`：[SelGate] ServerPort0=DBSrv(5100)、GatePort0=客户端(7100)；
    `LsvServerPort` 未被解析器读取（保持 16005 不动）。
  - `bins/game-gate/src/config.rs`：[GameGate] Count 组 ServerPortN=GameSvr(5000)、GatePortN=客户端(7200..)。
  - `bins/login-srv/src/config.rs`（路由）：
    `AddrTable.txt` = `ServerName Title Remote Public Gate[:Port]…`（GB2312），
    客户端收到的 SelGate 地址就是最后一列 `Gate`；
    `ServerAddr.txt` = 允许连 IDS(5600) 的白名单；`UserLimit.txt` = `ServerName Name LimitCountMax`。
  - `bins/db-srv/src/route.rs`（路由）：
    `AddrTable.txt` = 允许接入 5100/6000 的服务器 IP 白名单；
    `ServerInfo.txt` = `角色网关IP 游戏网关IP 端口[,端口…]`，选中行决定客户端去哪个 GameGate。

数据库目标
----------
默认 `--db-mode test`：三条 `ConnctionString=` 按**行级**改写为测试库，
即 db-srv → `mir2_test_db`、login-srv → `mir2_test_account`、game-svr → `mir2_test_data`。
注意 `_kv` 会把 `;` 当值后缀，不能做通用键替换；这里只在该行的 `database=` token 上替换。
生成后 fail-closed 扫描：窗口模式（test）下任一 conf 仍出现生产库名即报错。
`--db-mode prod` 保留源连接串（非窗口场景），并打印最终目标。

编码口径
--------
六份 conf 的源文件编码形态各异：`M2GameSvr/Server.conf` 有 UTF-8 BOM；仓库内
`src/DBSrv/bin/Release/dbsvr.conf` 与 `src/LoginSrv/bin/Release/logsrv.conf` **无 BOM**；
部署根 `DBServer/dbsvr.conf` 有 BOM，但它**不是默认源**（缺 MarketPort、地址不是本机）。
`RunGate/config.conf` 的 `[Strings]` 段实测有 `EF BC 3F`（既不是合法 UTF-8 也不是合法 GBK）。
本工具按 **latin-1 逐字节**读写（等价 `tools/m2-acceptance/db_target_plumb.ps1` 的做法），
只替换目标键的值 token：BOM、非 UTF-8 字节、CRLF、段序、注释全部 1:1 保留。
写完后用「差异行集合 == 计划键集合」做 fail-closed 校验。
路由文件（AddrTable/ServerInfo/ServerAddr/UserLimit）按行重建（地址/端口/白名单语义变化），
非 ASCII 字节（中文服务器名）原样搬运；每份写后都重新解析断言。

安全边界
--------
不启动任何进程、不占端口、不写数据库；`E:\\MirServer` 只读（deploy 只复制）。
客户端部分只读 `run-release.cmd`，只新增 `run-release-alt.cmd`。
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import shutil
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

sys.path.insert(0, str(Path(__file__).resolve().parent))
import startup_param as sp  # noqa: E402  （同目录工具）

DEFAULT_MIR_SERVER_ROOT = Path("E:/MirServer")
DEFAULT_CLIENT_RUN_DIR = Path("D:/MirClient-run")
DEFAULT_CLIENT_PORT = 17000
DEFAULT_CLIENT_ADDR = "127.0.0.1"

# deploy 覆盖层固定路由地址：本机窗口里所有角色/客户端都走回环。
ROUTE_ADDR = "127.0.0.1"
SELGATE_PORT = 17100
GAMEGATE_PORT_MAP: dict[int, int] = {7200: 17200, 7201: 17201, 7202: 17202}
GAMEGATE_PORTS_DEFAULT = "17200,17201,17202"

# 固定剖面（用于文档/`mapping` 子命令；实际改写值见 ROLE_SPECS）。
PROFILE: dict[str, str] = {
    "game-svr": "15000（GatePort；GameGate 下游）",
    "db-srv": "16000（ServerPort）/ 15100（GatePort）/ 15700（MarketPort）",
    "login-srv": "15500（GatePort）/ 15600（ServerPort）/ 16005（RunPort，legacy）",
    "login-gate": "17000（GatePort0；客户端入口）",
    "sel-gate": "17100（GatePort0；客户端入口）",
    "game-gate": "17200 / 17201 / 17202（GatePortN；保留 Count=3 间距）",
}

# 启动脚本的忙检/就绪等待端口（只列真正 listen 的端口；legacy RunPort 16005 不在内）。
PORTS_BY_ROLE: dict[str, list[int]] = {
    "db-srv": [16000, 15100, 15700],
    "login-srv": [15500, 15600],
    "game-svr": [15000],
    "login-gate": [17000],
    "sel-gate": [17100],
    "game-gate": [17200, 17201, 17202],
}

# role -> (源生产库, 测试库)
DB_TARGETS: dict[str, tuple[str, str]] = {
    "db-srv": ("mir2_db", "mir2_test_db"),
    "login-srv": ("mir2_account", "mir2_test_account"),
    "game-svr": ("mir2_data", "mir2_test_data"),
}


class ProfileError(RuntimeError):
    """源文件漂移、键缺失、差异校验失败等 fail-closed 错误。"""


@dataclass(frozen=True)
class Edit:
    section: str
    key: str
    new: str
    accepted_old: tuple[str, ...] = ()


@dataclass(frozen=True)
class RoleSpec:
    role: str
    deploy_dir: str  # 输出目录名（与 start_mir2_server.ps1 的 -Root 布局一致）
    mir_dir: str  # E:/MirServer 下的目录名
    filename: str
    edits: tuple[Edit, ...] = ()
    dynamic: str = ""  # "game-gate" 时按 Count 展开 ServerPortN/GatePortN

    def planned_edits(self, data: bytes) -> tuple[Edit, ...]:
        if self.dynamic != "game-gate":
            return self.edits
        count = _read_count(data, "GameGate") or 1
        out: list[Edit] = []
        for n in range(1, count + 1):
            out.append(
                Edit("GameGate", f"ServerPort{n}", "15000", ("5000",))
            )
            out.append(
                Edit(
                    "GameGate",
                    f"GatePort{n}",
                    str(17200 + n - 1),
                    (str(7200 + n - 1),),
                )
            )
        return tuple(out)


ROLE_SPECS: tuple[RoleSpec, ...] = (
    RoleSpec(
        role="db-srv",
        deploy_dir="DBSrv",
        mir_dir="DBServer",
        filename="dbsvr.conf",
        edits=(
            Edit("Setup", "ServerPort", "16000", ("6000",)),
            Edit("Setup", "GatePort", "15100", ("5100",)),
            Edit("Server", "MarketPort", "15700", ("5700",)),
            Edit("Server", "IDSPort", "15600", ("5600",)),
        ),
    ),
    RoleSpec(
        role="login-srv",
        deploy_dir="LoginSrv",
        mir_dir="LoginSrv",
        filename="logsrv.conf",
        edits=(
            Edit("Server", "GatePort", "15500", ("5500",)),
            Edit("Server", "ServerPort", "15600", ("5600",)),
            Edit("Server", "RunPort", "16005", ("7200", "16005")),
            Edit("Server", "DBSPort", "16000", ("6000", "16300")),
        ),
    ),
    RoleSpec(
        role="game-svr",
        deploy_dir="M2GameSvr",
        mir_dir="M2GameSvr",
        filename="Server.conf",
        edits=(
            Edit("Server", "GatePort", "15000", ("5000",)),
            Edit("Server", "DBPort", "16000", ("6000",)),
            Edit("Server", "IDSPort", "15600", ("5600",)),
        ),
    ),
    RoleSpec(
        role="login-gate",
        deploy_dir="LoginGate",
        mir_dir="LoginGate",
        filename="config.conf",
        edits=(
            Edit("LoginGate", "ServerPort0", "15500", ("5500",)),
            Edit("LoginGate", "GatePort0", "17000", ("7000",)),
        ),
    ),
    RoleSpec(
        role="sel-gate",
        deploy_dir="SelGate",
        mir_dir="SelGate",
        filename="config.conf",
        edits=(
            Edit("SelGate", "ServerPort0", "15100", ("5100",)),
            Edit("SelGate", "GatePort0", "17100", ("7100",)),
        ),
    ),
    RoleSpec(
        role="game-gate",
        deploy_dir="RunGate",
        mir_dir="RunGate",
        filename="config.conf",
        dynamic="game-gate",
    ),
)


# ---------------------------------------------------------------------------
# 字节级 INI 读写（latin-1 视图：BOM/非 UTF-8 字节/CRLF 全程 1:1 保留）
# ---------------------------------------------------------------------------


def _lines(data: bytes) -> list[bytes]:
    """按 \\n 切行（不动 0x85 等非 ASCII 字节，避开 str.splitlines 的多分隔符陷阱）。"""
    return data.split(b"\n")


def _body(line: bytes) -> tuple[bytes, bool]:
    if line.endswith(b"\r"):
        return line[:-1], True
    return line, False


def _section_key(line_body: bytes) -> str | None:
    stripped = line_body.strip()
    if stripped.startswith(b"\xef\xbb\xbf"):  # UTF-8 BOM 紧贴首行 `[DataBase]`（现网形态）
        stripped = stripped[3:]
    if len(stripped) >= 2 and stripped[:1] == b"[" and stripped[-1:] == b"]":
        return stripped[1:-1].decode("latin-1").strip()
    return None


def _kv(line_body: bytes) -> tuple[str, bytes, bytes, bytes] | None:
    """返回 (key, 等号前全部字节, 值 token, 值后缀)。后缀含空白的 `;;` 注释。"""
    eq = line_body.find(b"=")
    if eq < 0:
        return None
    key = line_body[:eq].strip().decode("latin-1")
    rest = line_body[eq + 1 :]
    m = re.match(rb"^([ \t]*)([^ \t;]*)(.*)$", rest, re.S)
    if not m:
        return None
    lead, token, suffix = m.groups()
    return key, line_body[: eq + 1] + lead, token, suffix


def _iter_kv(data: bytes) -> Iterable[tuple[int, str, str, bytes, bytes, bytes]]:
    """(行号, 段名, 键名, 前缀, token, 后缀)；段名保留原文大小写。"""
    section = ""
    for idx, line in enumerate(_lines(data)):
        body, _ = _body(line)
        name = _section_key(body)
        if name is not None:
            section = name
            continue
        kv = _kv(body)
        if kv is None:
            continue
        key, prefix, token, suffix = kv
        yield idx, section, key, prefix, token, suffix


def _read_count(data: bytes, section: str) -> int | None:
    for _, sec, key, _, token, _ in _iter_kv(data):
        if sec.lower() == section.lower() and key.lower() == "count":
            try:
                return int(token.decode("ascii"))
            except ValueError as exc:
                raise ProfileError(f"[{section}] Count 非法：{token!r}") from exc
    return None


def section_value(data: bytes, section: str, key: str) -> str | None:
    for _, sec, k, _, token, _ in _iter_kv(data):
        if sec.lower() == section.lower() and k.lower() == key.lower():
            return token.decode("latin-1")
    return None


def apply_edits(
    data: bytes, edits: tuple[Edit, ...], source: Path
) -> tuple[bytes, list[dict[str, Any]], list[dict[str, Any]]]:
    """只替换目标键的值 token；返回 (新字节, 实际变更, 已是目标值的 no-op)。"""
    lines = _lines(data)
    section = ""
    found: dict[tuple[str, str], str] = {}
    touched: dict[int, tuple[str, str, str]] = {}

    for idx, line in enumerate(lines):
        body, had_cr = _body(line)
        name = _section_key(body)
        if name is not None:
            section = name
            continue
        kv = _kv(body)
        if kv is None:
            continue
        key, prefix, token, suffix = kv
        for edit in edits:
            sig = (edit.section.lower(), edit.key.lower())
            if (section.lower(), key.lower()) != sig or sig in found:
                continue
            old = token.decode("latin-1")
            if edit.accepted_old and old not in edit.accepted_old:
                raise ProfileError(
                    f"{source}: [{edit.section}] {edit.key} 期望旧值 "
                    f"{'/'.join(edit.accepted_old)}，实际 {old!r}（源文件已漂移）"
                )
            found[sig] = old
            if old != edit.new:
                new_body = prefix + edit.new.encode("latin-1") + suffix
                lines[idx] = new_body + (b"\r" if had_cr else b"")
                touched[idx] = (edit.section, edit.key, old)
            break

    missing = [f"[{e.section}] {e.key}" for e in edits
               if (e.section.lower(), e.key.lower()) not in found]
    if missing:
        raise ProfileError(f"{source}: 缺目标键 {missing}")

    new_data = b"\n".join(lines)
    changes = [
        {"section": s, "key": k, "old": old, "new": _new_for(edits, s, k), "line": idx + 1}
        for idx, (s, k, old) in sorted(touched.items())
    ]
    noops = [
        {"section": s, "key": k, "old": old, "new": old}
        for (s, k), old in sorted(found.items())
        if (s, k) not in {(c["section"].lower(), c["key"].lower()) for c in changes}
    ]
    _verify_diff(data, new_data, touched, source)
    return new_data, changes, noops


def _new_for(edits: tuple[Edit, ...], section: str, key: str) -> str:
    for e in edits:
        if e.section.lower() == section.lower() and e.key.lower() == key.lower():
            return e.new
    raise ProfileError(f"内部错误：{section}/{key} 不在计划里")


def _verify_diff(
    old: bytes,
    new: bytes,
    touched: dict[int, tuple[str, str, str]],
    source: Path,
) -> None:
    """差异行集合必须恰为计划键所在行；差异行只允许值 token 变化。"""
    old_lines, new_lines = _lines(old), _lines(new)
    if len(old_lines) != len(new_lines):
        raise ProfileError(f"{source}: 行数变化 {len(old_lines)} -> {len(new_lines)}")
    changed = {i for i, (a, b) in enumerate(zip(old_lines, new_lines)) if a != b}
    if changed != set(touched):
        raise ProfileError(
            f"{source}: 差异行 {sorted(i + 1 for i in changed)} "
            f"!= 计划行 {sorted(i + 1 for i in touched)}"
        )
    for idx in changed:
        ob, _ = _body(old_lines[idx])
        nb, _ = _body(new_lines[idx])
        okv, nkv = _kv(ob), _kv(nb)
        if okv is None or nkv is None:
            raise ProfileError(f"{source}:{idx + 1} 不是键值行")
        if okv[1] != nkv[1] or okv[3] != nkv[3]:
            raise ProfileError(f"{source}:{idx + 1} 值 token 之外有字节变化")


def _verify_output(data: bytes, edits: tuple[Edit, ...], out: Path) -> None:
    for edit in edits:
        actual = section_value(data, edit.section, edit.key)
        if actual != edit.new:
            raise ProfileError(
                f"{out}: [{edit.section}] {edit.key} 期望 {edit.new}，实际 {actual!r}"
            )


# ---------------------------------------------------------------------------
# 连接串：行级改写 `database=` token（不能做通用键替换：`;` 是值分隔符）
# ---------------------------------------------------------------------------

_DB_TOKEN = re.compile(rb"(database=)([A-Za-z0-9_]+)(;)")
_PROD_DB_SCAN = re.compile(rb"database=(mir2_db|mir2_account|mir2_data)(?=;)")


def _conn_string_line(data: bytes, source: Path) -> tuple[int, re.Match[bytes]]:
    hits: list[tuple[int, re.Match[bytes]]] = []
    for idx, line in enumerate(_lines(data)):
        body, _ = _body(line)
        kv = _kv(body)
        if kv is None:
            continue
        if kv[0].lower() != "connctionstring":
            continue
        m = _DB_TOKEN.search(body)
        if not m:
            raise ProfileError(
                f"{source}:{idx + 1} ConnctionString 里找不到 database=…;"
            )
        hits.append((idx, m))
    if len(hits) != 1:
        raise ProfileError(
            f"{source}: ConnctionString 行数 {len(hits)}，期望恰 1 行"
        )
    return hits[0]


def rewrite_conn_string(
    data: bytes, role: str, mode: str, source: Path
) -> tuple[bytes, dict[str, Any]]:
    """把 ConnctionString 的 `database=` token 改成 --db-mode 指定库（只动该 token）。

    `--db-mode prod` 的语义是**保留源连接串**（不反向改写测试库）。
    """
    prod, test = DB_TARGETS[role]
    idx, m = _conn_string_line(data, source)
    current = m.group(2).decode("latin-1")
    if current not in (prod, test):
        raise ProfileError(
            f"{source}:{idx + 1} database={current} 不在已知目标 "
            f"({prod}/{test})，源文件已漂移"
        )
    if mode == "prod":
        return data, {
            "role": role,
            "section": "DataBase",
            "key": "ConnctionString",
            "old": current,
            "new": current,
            "line": idx + 1,
            "mode": mode,
            "noop": True,
        }
    target = test
    info = {
        "role": role,
        "section": "DataBase",
        "key": "ConnctionString",
        "old": current,
        "new": target,
        "line": idx + 1,
        "mode": mode,
        "noop": current == target,
    }
    if current == target:
        return data, info

    lines = _lines(data)
    line = lines[idx]
    body, had_cr = _body(line)
    new_body = body[: m.start(2)] + target.encode("latin-1") + body[m.end(2):]
    lines[idx] = new_body + (b"\r" if had_cr else b"")
    new_data = b"\n".join(lines)

    old_lines = _lines(data)
    changed = {i for i, (a, b) in enumerate(zip(old_lines, lines)) if a != b}
    if changed != {idx}:
        raise ProfileError(
            f"{source}: 连接串改写差异行 {sorted(i + 1 for i in changed)} != {idx + 1}"
        )
    ob, _ = _body(old_lines[idx])
    nb, _ = _body(lines[idx])
    mo, mn = _DB_TOKEN.search(ob), _DB_TOKEN.search(nb)
    if mo is None or mn is None:
        raise ProfileError(f"{source}:{idx + 1} 连接串 token 丢失")
    if ob[: mo.start(2)] != nb[: mn.start(2)] or ob[mo.end(2):] != nb[mn.end(2):]:
        raise ProfileError(f"{source}:{idx + 1} database token 之外有字节变化")
    return new_data, info


def assert_no_prod_db(data: bytes, path: Path) -> None:
    """窗口模式 fail-closed：输出里出现生产库名即报错。"""
    m = _PROD_DB_SCAN.search(data)
    if m:
        raise ProfileError(
            f"{path}: 窗口模式仍是生产库 database={m.group(1).decode('latin-1')}；"
            "请用 --db-mode test"
        )


def _verify_db_output(data: bytes, role: str, mode: str, out: Path) -> None:
    prod, test = DB_TARGETS[role]
    idx, m = _conn_string_line(data, out)
    actual = m.group(2).decode("latin-1")
    if mode == "prod":
        # prod = 保留源连接串：只要求仍是一个已知目标，不做反向改写。
        if actual not in (prod, test):
            raise ProfileError(
                f"{out}:{idx + 1} database={actual} 不在已知目标（{prod}/{test}）"
            )
        return
    if actual != test:
        raise ProfileError(
            f"{out}:{idx + 1} database 期望 {test}，实际 {actual!r}"
        )
    assert_no_prod_db(data, out)


# ---------------------------------------------------------------------------
# 路由文件（AddrTable / ServerInfo / ServerAddr / UserLimit）
# ---------------------------------------------------------------------------


def _tokens(line: bytes) -> list[bytes]:
    return [t for t in re.split(rb"[ \t]+", line.strip()) if t]


def rewrite_addr_table(data: bytes, source: Path) -> tuple[bytes, dict[str, Any]]:
    """LoginSrv/AddrTable.txt：Remote/Public/Gate 全部指向本机 127.0.0.1，
    SelGate 端口改成 17100。ServerName/Title（含 GBK 字节）原样保留。"""
    out: list[bytes] = []
    server_name: bytes | None = None
    routes = 0
    for line in _lines(data):
        body, had_cr = _body(line)
        stripped = body.strip()
        if not stripped or stripped.startswith(b";"):
            out.append(line)
            continue
        toks = _tokens(stripped)
        if len(toks) < 5:
            raise ProfileError(
                f"{source}: AddrTable 行列数 {len(toks)} < 5（需 ServerName Title Remote Public Gate）"
            )
        name, title = toks[0], toks[1]
        if server_name is None:
            server_name = name
        new = [name, title, ROUTE_ADDR.encode(), ROUTE_ADDR.encode()]
        for gate in toks[4:]:
            disabled = gate.startswith(b"*")
            token = gate[1:] if disabled else gate
            ip, sep, _port = token.partition(b":")
            if not sep or not ip:
                raise ProfileError(
                    f"{source}: AddrTable gate token 非法：{gate!r}（需 IP:Port）"
                )
            prefix = b"*" if disabled else b""
            new.append(prefix + f"{ROUTE_ADDR}:{SELGATE_PORT}".encode())
        out.append(b" ".join(new) + (b"\r" if had_cr else b""))
        routes += 1
    if routes == 0:
        raise ProfileError(f"{source}: AddrTable 没有有效路由行（login-srv 起不来）")
    new_data = b"\n".join(out)
    _verify_addr_table(new_data, source)
    return new_data, {
        "file": "LoginSrv/AddrTable.txt",
        "source": str(source),
        "routes": routes,
        "server_name_hex": (server_name or b"").hex(),
        "selgate": f"{ROUTE_ADDR}:{SELGATE_PORT}",
    }


def _parse_addr_routes(data: bytes, source: Path) -> list[list[bytes]]:
    routes: list[list[bytes]] = []
    for line in _lines(data):
        body, _ = _body(line)
        stripped = body.strip()
        if not stripped or stripped.startswith(b";"):
            continue
        toks = _tokens(stripped)
        if len(toks) < 5:
            raise ProfileError(f"{source}: AddrTable 行列数 < 5：{stripped!r}")
        routes.append(toks)
    return routes


def _verify_addr_table(data: bytes, source: Path) -> None:
    routes = _parse_addr_routes(data, source)
    if not routes:
        raise ProfileError(f"{source}: AddrTable 无路由")
    for toks in routes:
        if toks[2] != ROUTE_ADDR.encode() or toks[3] != ROUTE_ADDR.encode():
            raise ProfileError(f"{source}: Remote/Public 未指向 {ROUTE_ADDR}: {toks[2:4]!r}")
        gates = []
        for gate in toks[4:]:
            token = gate[1:] if gate.startswith(b"*") else gate
            ip, sep, port = token.partition(b":")
            if not sep or ip.decode("latin-1") != ROUTE_ADDR:
                raise ProfileError(f"{source}: gate 未指向 {ROUTE_ADDR}：{gate!r}")
            gates.append(port)
        if not gates or any(p != str(SELGATE_PORT).encode() for p in gates):
            raise ProfileError(f"{source}: gate 端口不是 {SELGATE_PORT}：{gates!r}")


def rewrite_server_info(data: bytes, source: Path) -> tuple[bytes, dict[str, Any]]:
    """DBSrv/ServerInfo.txt：`角色网关IP 游戏网关IP 端口[,端口…]` 全部指向本机，
    GameGate 端口按 7200/7201/7202 → 17200/17201/17202 平移。"""
    out: list[bytes] = []
    routes = 0
    ports_all: list[int] = []
    for line in _lines(data):
        body, had_cr = _body(line)
        stripped = body.strip()
        if not stripped or stripped.startswith(b";"):
            out.append(line)
            continue
        toks = _tokens(stripped)
        if len(toks) < 3:
            raise ProfileError(
                f"{source}: ServerInfo 行列数 {len(toks)} < 3（需 SelGateIP GameGateIP 端口）"
            )
        new_ports: list[str] = []
        for tok in toks[2:]:
            for piece in tok.split(b","):
                try:
                    old_port = int(piece.decode("ascii"))
                except ValueError as exc:
                    raise ProfileError(
                        f"{source}: ServerInfo 端口非法：{piece!r}"
                    ) from exc
                if old_port not in GAMEGATE_PORT_MAP:
                    raise ProfileError(
                        f"{source}: ServerInfo 端口 {old_port} 不在剖面映射 "
                        f"{sorted(GAMEGATE_PORT_MAP)}（源文件已漂移）"
                    )
                new_ports.append(str(GAMEGATE_PORT_MAP[old_port]))
                ports_all.append(GAMEGATE_PORT_MAP[old_port])
        out.append(
            b" ".join([ROUTE_ADDR.encode(), ROUTE_ADDR.encode(), ",".join(new_ports).encode()])
            + (b"\r" if had_cr else b"")
        )
        routes += 1
    if routes == 0:
        out = [f"{ROUTE_ADDR} {ROUTE_ADDR} {GAMEGATE_PORTS_DEFAULT}".encode(), b""]
        ports_all = [int(p) for p in GAMEGATE_PORTS_DEFAULT.split(",")]
        routes = 1
    new_data = b"\n".join(out)
    _verify_server_info(new_data, source)
    return new_data, {
        "file": "DBSrv/ServerInfo.txt",
        "source": str(source),
        "routes": routes,
        "selgate": ROUTE_ADDR,
        "gamegate_ports": sorted(set(ports_all)),
        "generated": routes == 1 and not data.strip(),
    }


def _parse_server_info(data: bytes, source: Path) -> list[list[bytes]]:
    routes: list[list[bytes]] = []
    for line in _lines(data):
        body, _ = _body(line)
        stripped = body.strip()
        if not stripped or stripped.startswith(b";"):
            continue
        toks = _tokens(stripped)
        if len(toks) < 3:
            raise ProfileError(f"{source}: ServerInfo 行列数 < 3：{stripped!r}")
        routes.append(toks)
    return routes


def _verify_server_info(data: bytes, source: Path) -> None:
    routes = _parse_server_info(data, source)
    if not routes:
        raise ProfileError(f"{source}: ServerInfo 无路由")
    for toks in routes:
        if toks[0] != ROUTE_ADDR.encode() or toks[1] != ROUTE_ADDR.encode():
            raise ProfileError(f"{source}: ServerInfo IP 未指向 {ROUTE_ADDR}: {toks[:2]!r}")
        ports = [p for tok in toks[2:] for p in tok.split(b",")]
        if b"17200" not in ports:
            raise ProfileError(f"{source}: ServerInfo 未路由到 GameGate 17200：{ports!r}")


def rewrite_ip_whitelist(data: bytes, source: Path) -> tuple[bytes, dict[str, Any]]:
    """DBSrv/AddrTable.txt 与 LoginSrv/ServerAddr.txt：本机窗口只需要 127.0.0.1。"""
    kept = [
        line.strip()
        for line in _lines(data)
        if line.strip() and not line.strip().startswith(b";")
    ]
    return (
        f"{ROUTE_ADDR}\r\n".encode(),
        {
            "source": str(source),
            "old_entries": len(kept),
            "new": ROUTE_ADDR,
        },
    )


def rewrite_user_limit(
    data: bytes | None, server_name: bytes, source: Path
) -> tuple[bytes, dict[str, Any]]:
    """LoginSrv/UserLimit.txt：首列必须与 AddrTable 的 ServerName 对齐（缺文件则补默认）。"""
    if data is None or not data.strip():
        new_data = server_name + b" " + server_name + b" 10000\r\n"
        return new_data, {
            "source": str(source),
            "generated": True,
            "server_name_hex": server_name.hex(),
        }
    out: list[bytes] = []
    lines = 0
    for line in _lines(data):
        body, had_cr = _body(line)
        stripped = body.strip()
        if not stripped or stripped.startswith(b";"):
            out.append(line)
            continue
        toks = _tokens(stripped)
        if len(toks) < 3:
            raise ProfileError(
                f"{source}: UserLimit 行列数 {len(toks)} < 3（需 ServerName Name Limit）"
            )
        out.append(b" ".join([server_name] + toks[1:]) + (b"\r" if had_cr else b""))
        lines += 1
    if lines == 0:
        out = [server_name + b" " + server_name + b" 10000", b""]
    return b"\n".join(out), {
        "source": str(source),
        "generated": lines == 0,
        "server_name_hex": server_name.hex(),
    }


# ---------------------------------------------------------------------------
# 源文件解析与生成
# ---------------------------------------------------------------------------


def resolve_source(
    spec: RoleSpec,
    repo_root: Path,
    mir_root: Path,
    overrides: dict[str, Path],
) -> Path:
    if spec.role in overrides:
        p = overrides[spec.role]
        if not p.is_file():
            raise ProfileError(f"--source {spec.role}={p} 不存在")
        return p
    if spec.role == "db-srv":
        # 仓库内这两份是 Rust 口径（键名/连接串）；部署根旧文件缺 MarketPort 且地址不是本机，
        # 不拿来做默认源（见 runbook §7.2「部署布局提示」）。
        candidates = [repo_root / "src/DBSrv/bin/Release/dbsvr.conf"]
        hint = "（--repo-root 指向主检出，或 --source db-srv=<path>）"
    elif spec.role == "login-srv":
        candidates = [repo_root / "src/LoginSrv/bin/Release/logsrv.conf"]
        hint = "（--repo-root 指向主检出，或 --source login-srv=<path>）"
    else:
        candidates = [mir_root / spec.mir_dir / spec.filename]
        hint = ""
    for p in candidates:
        if p.is_file():
            return p
    raise ProfileError(
        f"{spec.role}: 找不到源配置，候选 {[str(p) for p in candidates]}{hint}"
    )


def generate_confs(
    out_dir: Path,
    repo_root: Path,
    mir_root: Path,
    overrides: dict[str, Path],
    what_if: bool = False,
    db_mode: str = "test",
) -> dict[str, Any]:
    if db_mode not in ("test", "prod"):
        raise ProfileError(f"未知 --db-mode：{db_mode}")
    outputs: dict[str, Any] = {}
    port_map: list[dict[str, Any]] = []
    db_targets: dict[str, Any] = {}
    for spec in ROLE_SPECS:
        source = resolve_source(spec, repo_root, mir_root, overrides)
        data = source.read_bytes()
        edits = spec.planned_edits(data)
        if what_if:
            changed: list[dict[str, Any]] = []
            for edit in edits:
                old = section_value(data, edit.section, edit.key)
                if old is None:
                    raise ProfileError(
                        f"{source}: 缺目标键 [{edit.section}] {edit.key}"
                    )
                if old != edit.new:
                    changed.append(
                        {"section": edit.section, "key": edit.key, "old": old, "new": edit.new, "line": 0}
                    )
            new_data = data
            noops = [
                {"section": e.section, "key": e.key, "old": e.new, "new": e.new}
                for e in edits
                if section_value(data, e.section, e.key) == e.new
            ]
        else:
            new_data, changed, noops = apply_edits(data, edits, source)

        db_change: dict[str, Any] | None = None
        if spec.role in DB_TARGETS:
            new_data, db_info = rewrite_conn_string(new_data, spec.role, db_mode, source)
            db_targets[spec.role] = db_info
            if db_info["old"] != db_info["new"]:
                db_change = {
                    "section": db_info["section"],
                    "key": f"{db_info['key']} (database)",
                    "old": db_info["old"],
                    "new": db_info["new"],
                    "line": db_info["line"],
                    "kind": "db",
                }
                changed.append(db_change)
            else:
                noops.append(
                    {
                        "section": db_info["section"],
                        "key": f"{db_info['key']} (database)",
                        "old": db_info["new"],
                        "new": db_info["new"],
                    }
                )

        out = out_dir / spec.deploy_dir / spec.filename
        if not what_if:
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_bytes(new_data)
            _verify_output(new_data, edits, out)
            if spec.role in DB_TARGETS:
                _verify_db_output(new_data, spec.role, db_mode, out)
            elif db_mode == "test":
                # 六份输出统一兜底：任何 conf 里出现生产库名都 RED。
                assert_no_prod_db(new_data, out)

        for c in changed:
            if c.get("kind") == "db":
                continue
            port_map.append(
                {
                    "role": spec.role,
                    "section": c["section"],
                    "key": c["key"],
                    "old": c["old"],
                    "new": c["new"],
                }
            )
        outputs[spec.role] = {
            "source": str(source),
            "output": str(out),
            "changed": changed,
            "noops": noops,
            "diff_set": sorted(
                f"[{c['section']}] {c['key']}" for c in changed
            ),
        }
    return {"outputs": outputs, "port_map": port_map, "db_targets": db_targets}


# ---------------------------------------------------------------------------
# deploy：复制角色树 + 改写路由文件
# ---------------------------------------------------------------------------


def _count_files(root: Path) -> int:
    return sum(1 for p in root.rglob("*") if p.is_file())


def copy_role_trees(
    mir_root: Path, deploy_root: Path, force: bool, what_if: bool
) -> dict[str, Any]:
    if deploy_root.exists() and any(deploy_root.iterdir()) and not force:
        raise ProfileError(
            f"{deploy_root} 已存在且非空；确认覆盖请加 --force（源目录只读，不会被改）"
        )
    entries: list[dict[str, Any]] = []
    for spec in ROLE_SPECS:
        src = mir_root / spec.mir_dir
        dst = deploy_root / spec.deploy_dir
        if not src.is_dir():
            raise ProfileError(f"--deploy 缺少角色目录：{src}")
        files = _count_files(src)
        if not what_if:
            dst.mkdir(parents=True, exist_ok=True)
            shutil.copytree(src, dst, dirs_exist_ok=True)
        entries.append(
            {
                "role": spec.role,
                "source": str(src),
                "output": str(dst),
                "files": files,
            }
        )
    return {"root": str(deploy_root), "roles": entries}


def deploy_routes(
    mir_root: Path, deploy_root: Path, what_if: bool
) -> dict[str, Any]:
    db_src = mir_root / "DBServer"
    lg_src = mir_root / "LoginSrv"

    def read_opt(path: Path) -> bytes | None:
        return path.read_bytes() if path.is_file() else None

    addr_src = read_opt(lg_src / "AddrTable.txt")
    if addr_src is None:
        raise ProfileError(
            f"deploy 需要一个可用的 LoginSrv/AddrTable.txt：{lg_src / 'AddrTable.txt'} 不存在"
        )
    addr_new, addr_info = rewrite_addr_table(addr_src, lg_src / "AddrTable.txt")
    server_name = bytes.fromhex(addr_info["server_name_hex"])

    info_src = read_opt(db_src / "ServerInfo.txt")
    info_new, info_info = rewrite_server_info(
        info_src or b"", db_src / "ServerInfo.txt"
    )
    db_addr_src = read_opt(db_src / "AddrTable.txt")
    db_addr_new, db_addr_info = rewrite_ip_whitelist(
        db_addr_src or b"", db_src / "AddrTable.txt"
    )
    lg_saddr_src = read_opt(lg_src / "ServerAddr.txt")
    lg_saddr_new, lg_saddr_info = rewrite_ip_whitelist(
        lg_saddr_src or b"", lg_src / "ServerAddr.txt"
    )
    ulimit_src = read_opt(lg_src / "UserLimit.txt")
    ulimit_new, ulimit_info = rewrite_user_limit(
        ulimit_src, server_name, lg_src / "UserLimit.txt"
    )

    files: dict[str, bytes] = {
        "DBSrv/AddrTable.txt": db_addr_new,
        "DBSrv/ServerInfo.txt": info_new,
        "LoginSrv/AddrTable.txt": addr_new,
        "LoginSrv/ServerAddr.txt": lg_saddr_new,
        "LoginSrv/UserLimit.txt": ulimit_new,
    }
    if not what_if:
        for rel, payload in files.items():
            path = deploy_root / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(payload)

    verify = verify_deploy_routes(files)
    return {
        "files": {rel: str(deploy_root / rel) for rel in files},
        "addr_table": addr_info,
        "server_info": info_info,
        "db_addr_table": db_addr_info,
        "server_addr": lg_saddr_info,
        "user_limit": ulimit_info,
        "verify": verify,
    }


def verify_deploy_routes(files: dict[str, bytes]) -> dict[str, Any]:
    """端到端断言：客户端登录 17000 → 选服 17100 → 游戏 17200。"""
    addr = files["LoginSrv/AddrTable.txt"]
    info = files["DBSrv/ServerInfo.txt"]
    db_addr = files["DBSrv/AddrTable.txt"]
    server_addr = files["LoginSrv/ServerAddr.txt"]
    _verify_addr_table(addr, Path("LoginSrv/AddrTable.txt"))
    _verify_server_info(info, Path("DBSrv/ServerInfo.txt"))
    for name, payload in (
        ("DBSrv/AddrTable.txt", db_addr),
        ("LoginSrv/ServerAddr.txt", server_addr),
    ):
        entries = [
            line.strip()
            for line in _lines(payload)
            if line.strip() and not line.strip().startswith(b";")
        ]
        if entries != [ROUTE_ADDR.encode()]:
            raise ProfileError(f"{name}: 白名单应恰为 {ROUTE_ADDR}，实际 {entries!r}")
    return {
        "client_login": f"{ROUTE_ADDR}:17000",
        "selgate": f"{ROUTE_ADDR}:{SELGATE_PORT}",
        "gamegate": f"{ROUTE_ADDR}:17200",
        "db_whitelist": ROUTE_ADDR,
        "ids_whitelist": ROUTE_ADDR,
        "ok": True,
    }


def deploy_stack(
    deploy_root: Path,
    repo_root: Path,
    mir_root: Path,
    overrides: dict[str, Path],
    db_mode: str,
    force: bool,
    what_if: bool,
) -> dict[str, Any]:
    copy_info = copy_role_trees(mir_root, deploy_root, force=force, what_if=what_if)
    confs = generate_confs(
        deploy_root, repo_root, mir_root, overrides, what_if=what_if, db_mode=db_mode
    )
    routes = deploy_routes(mir_root, deploy_root, what_if=what_if)
    if not what_if:
        reread = {
            rel: (deploy_root / rel).read_bytes()
            for rel in routes["files"]
        }
        routes["verify_after_write"] = verify_deploy_routes(reread)
    return {"copy": copy_info, "confs": confs, "routes": routes}


# ---------------------------------------------------------------------------
# 客户端启动参数（只新增 run-release-alt.cmd）
# ---------------------------------------------------------------------------


def find_param_token(text: str) -> str:
    m = re.search(r"[\"']([A-Za-z0-9+/=]{64,})[\"']", text)
    if m:
        return m.group(1)
    m = re.search(r"\b([A-Za-z0-9+/=]{64,})\b", text)
    if not m:
        raise ProfileError("run-release.cmd 里找不到 Base64 启动参数")
    return m.group(1)


def build_client_param(
    source_param: str, port: int = DEFAULT_CLIENT_PORT,
    addr: str = DEFAULT_CLIENT_ADDR, iv_mode: str = "preserve",
) -> str:
    plain = sp.decrypt(source_param)
    plain = sp.patch_addr(plain, addr)
    plain = sp.patch_port(plain, port)
    if iv_mode == "preserve":
        iv = base64.b64decode(source_param, validate=True)[: sp.IV_LEN]
    elif iv_mode == "random":
        iv = os.urandom(sp.IV_LEN)
    else:
        raise ProfileError(f"未知 --iv 模式：{iv_mode}")
    return sp.encrypt(plain, iv)


def generate_client_cmd(
    client_run_dir: Path,
    port: int = DEFAULT_CLIENT_PORT,
    addr: str = DEFAULT_CLIENT_ADDR,
    iv_mode: str = "preserve",
    force: bool = False,
    what_if: bool = False,
) -> dict[str, Any]:
    source = client_run_dir / "run-release.cmd"
    out = client_run_dir / "run-release-alt.cmd"
    if not source.is_file():
        raise ProfileError(f"找不到客户端启动命令：{source}")
    original = source.read_bytes()
    original_hash = hashlib.sha256(original).hexdigest()
    text = original.decode("latin-1")
    token = find_param_token(text)

    plain = sp.decrypt(token)
    old_port = int.from_bytes(plain[sp.PORT_OFFSET : sp.PORT_OFFSET + 4], "little", signed=True)
    old_addr = sp.read_short(plain, sp.ADDR_OFFSET, sp.ADDR_CAPACITY).decode("gbk", "replace")
    new_token = build_client_param(token, port=port, addr=addr, iv_mode=iv_mode)

    new_text = text.replace(token, new_token, 1)
    new_text = new_text.replace(f"{old_addr}:{old_port}", f"{addr}:{port}")
    marker = (
        "rem ALT-PORT profile (generated by tools/window/alt_port_profile.py): "
        f"login-gate {addr}:{port}; source run-release.cmd unchanged\r\n"
    )
    if "ALT-PORT profile" not in new_text:
        if "@echo off\r\n" in new_text:
            new_text = new_text.replace("@echo off\r\n", "@echo off\r\n" + marker, 1)
        elif "@echo off\n" in new_text:
            new_text = new_text.replace("@echo off\n", "@echo off\n" + marker, 1)
        else:
            new_text = marker + new_text

    if not what_if:
        if out.exists() and not force:
            raise ProfileError(f"{out} 已存在；确认要覆盖请加 --force（原 run-release.cmd 不受影响）")
        out.write_bytes(new_text.encode("latin-1"))
        verify = verify_client_cmd(out, port=port, addr=addr)
        if source.read_bytes() != original or hashlib.sha256(
            source.read_bytes()
        ).hexdigest() != original_hash:
            raise ProfileError(f"原文件被改动了：{source}")
    else:
        verify = {"note": "what-if：未写文件"}

    return {
        "source": str(source),
        "output": str(out),
        "source_sha256": original_hash,
        "old_addr": old_addr,
        "old_port": old_port,
        "new_addr": addr,
        "new_port": port,
        "iv_mode": iv_mode,
        "verify": verify,
    }


def verify_client_cmd(path: Path, port: int, addr: str) -> dict[str, Any]:
    text = path.read_bytes().decode("latin-1")
    token = find_param_token(text)
    plain = sp.decrypt(token)
    actual_port = int.from_bytes(
        plain[sp.PORT_OFFSET : sp.PORT_OFFSET + 4], "little", signed=True
    )
    actual_addr = sp.read_short(plain, sp.ADDR_OFFSET, sp.ADDR_CAPACITY).decode("gbk")
    if actual_port != port or actual_addr != addr:
        raise ProfileError(
            f"{path}: 解密回 {actual_addr}:{actual_port}，期望 {addr}:{port}"
        )
    return {
        "param_len": len(token),
        "decrypted_addr": actual_addr,
        "decrypted_port": actual_port,
    }


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _parse_overrides(pairs: list[str]) -> dict[str, Path]:
    out: dict[str, Path] = {}
    for item in pairs or []:
        if "=" not in item:
            raise ProfileError(f"--source 需要 role=path：{item!r}")
        role, path = item.split("=", 1)
        if role not in {s.role for s in ROLE_SPECS}:
            raise ProfileError(f"未知角色 {role!r}")
        out[role] = Path(path)
    return out


def _print_mapping(result: dict[str, Any]) -> None:
    for role, info in result["outputs"].items():
        print(f"SOURCE {role} -> {info['source']}")
        if not info["changed"]:
            print(f"  {role}: 无变化（已是目标值）")
        for c in info["changed"]:
            line = f":{c['line']}" if c.get("line") else ""
            print(
                f"  [{c['section']}] {c['key']}{line}: {c['old']} -> {c['new']}"
            )
        if info.get("noops"):
            for n in info["noops"]:
                print(f"  [{n['section']}] {n['key']}: 已是 {n['new']}（no-op）")
        print(f"VERIFY {role}: diff_set={info['diff_set']} OK")
    for role, info in result.get("db_targets", {}).items():
        prod, test = DB_TARGETS[role]
        print(
            f"DB_TARGET {role}: database={info['new']} (mode={info['mode']}, "
            f"源 {prod}, 测试 {test})"
        )


def _print_routes(routes: dict[str, Any]) -> None:
    for rel, path in routes["files"].items():
        print(f"ROUTE {rel} -> {path}")
    a = routes["addr_table"]
    print(
        f"ROUTE LoginSrv/AddrTable.txt: {a['routes']} 条；"
        f"Remote/Public={ROUTE_ADDR}；client->selgate={a['selgate']}"
    )
    s = routes["server_info"]
    print(
        f"ROUTE DBSrv/ServerInfo.txt: {s['routes']} 条；"
        f"selgate={s['selgate']}；gamegate ports={s['gamegate_ports']}"
    )
    u = routes["user_limit"]
    print(f"ROUTE LoginSrv/UserLimit.txt: server_name_hex={u['server_name_hex']}")
    v = routes.get("verify_after_write") or routes["verify"]
    print(
        f"ROUTE_VERIFY OK client_login={v['client_login']} -> selgate={v['selgate']} "
        f"-> gamegate={v['gamegate']} (db/ids whitelist {v['db_whitelist']})"
    )


def cmd_generate(args: argparse.Namespace) -> int:
    repo_root = Path(args.repo_root).resolve()
    mir_root = Path(args.mir_server_root)
    overrides = _parse_overrides(args.source)

    deploy_dir = Path(args.deploy) if args.deploy else None
    if deploy_dir is not None and args.out_dir:
        raise ProfileError("--deploy 与 --out-dir 互斥（deploy 自带输出根）")
    if deploy_dir is None and not args.out_dir:
        raise ProfileError("请给 --out-dir（覆盖层）或 --deploy DIR（完整部署根）")
    out_dir = deploy_dir or Path(args.out_dir)

    deploy_info: dict[str, Any] | None = None
    if deploy_dir is not None:
        deploy_info = deploy_stack(
            out_dir,
            repo_root,
            mir_root,
            overrides,
            db_mode=args.db_mode,
            force=args.force,
            what_if=args.what_if,
        )
        result = deploy_info["confs"]
        _print_mapping(result)
        for entry in deploy_info["copy"]["roles"]:
            print(
                f"DEPLOY {entry['role']}: {entry['source']} -> {entry['output']} "
                f"({entry['files']} files，源只读)"
            )
        _print_routes(deploy_info["routes"])
    else:
        result = generate_confs(
            out_dir, repo_root, mir_root, overrides,
            what_if=args.what_if, db_mode=args.db_mode,
        )
        _print_mapping(result)
        print(
            "OVERLAY: 只生成六份 conf，不是可独立起栈的 -Root；"
            "需与完整角色部署合并，或用 --deploy 生成完整根"
        )

    client: dict[str, Any] | None = None
    if args.client_run_dir:
        client = generate_client_cmd(
            Path(args.client_run_dir),
            port=args.client_port,
            addr=args.client_addr,
            iv_mode=args.iv,
            force=args.force,
            what_if=args.what_if,
        )
        print(
            f"CLIENT {client['source']} ({client['old_addr']}:{client['old_port']}) "
            f"-> {client['output']} ({client['new_addr']}:{client['new_port']})"
        )
        if client["verify"].get("decrypted_port") is not None:
            v = client["verify"]
            print(
                f"VERIFY client: decrypt_port={v['decrypted_port']} "
                f"decrypt_addr={v['decrypted_addr']} param_len={v['param_len']} OK"
            )

    manifest = {
        "profile": "alt-port-15xxx-17xxx",
        "mode": "deploy" if deploy_dir is not None else "overlay",
        "mir_server_root": str(mir_root),
        "repo_root": str(repo_root),
        "out_dir": str(out_dir),
        "db_mode": args.db_mode,
        "port_profile": PROFILE,
        "ports_by_role": PORTS_BY_ROLE,
        "db_targets": result["db_targets"],
        "outputs": result["outputs"],
        "port_map": result["port_map"],
        "deploy": deploy_info,
        "routes": deploy_info["routes"] if deploy_info else None,
        "client": client,
    }
    if not args.what_if:
        out_dir.mkdir(parents=True, exist_ok=True)
        (out_dir / "alt-port-profile.json").write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
            newline="\n",
        )
    if args.json:
        print(json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True))
    print("WHATIF OK（未写文件）" if args.what_if else "ALT_PORT_PROFILE_OK")
    return 0


def cmd_mapping(args: argparse.Namespace) -> int:
    repo_root = Path(args.repo_root).resolve()
    mir_root = Path(args.mir_server_root)
    overrides = _parse_overrides(args.source)
    print("角色固定剖面：")
    for role, desc in PROFILE.items():
        print(f"  {role:<11} {desc}")
    print("\n计划键（实际旧值以源文件为准）：")
    for spec in ROLE_SPECS:
        source = resolve_source(spec, repo_root, mir_root, overrides)
        data = source.read_bytes()
        for edit in spec.planned_edits(data):
            old = section_value(data, edit.section, edit.key)
            if old is None:
                flag = "  MISSING（源文件缺该键）"
            elif old == edit.new:
                flag = "  （已是目标值，no-op）"
            else:
                flag = f"  {old} -> {edit.new}"
            print(
                f"  {spec.role:<11} [{edit.section}] {edit.key}={edit.new}{flag}"
                f"  (源: {source})"
            )
        if spec.role in DB_TARGETS:
            prod, test = DB_TARGETS[spec.role]
            print(
                f"  {spec.role:<11} [DataBase] ConnctionString: database={prod} -> {test}"
                f"（--db-mode test；prod 则不改）  (源: {source})"
            )
    print("\n路由文件（--deploy 改写；客户端 17000 -> 17100 -> 17200）：")
    for rel in (
        "LoginSrv/AddrTable.txt",
        "LoginSrv/ServerAddr.txt",
        "LoginSrv/UserLimit.txt",
        "DBSrv/AddrTable.txt",
        "DBSrv/ServerInfo.txt",
    ):
        print(f"  {rel} -> {ROUTE_ADDR}（SelGate {SELGATE_PORT} / GameGate 17200）")
    return 0


def _selftest_synthetic_root(tmp: Path, repo_root: Path, mir_root: Path) -> Path:
    """构造一个小的假 -MirServer：六角色配置 + 路由文件（真实文件可用时优先复制）。"""
    fake = tmp / "mirserver"
    for spec in ROLE_SPECS:
        src = resolve_source(spec, repo_root, mir_root, {})
        dst = fake / spec.mir_dir / spec.filename
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
        (fake / spec.mir_dir / "marker.txt").write_text("tree-copy\n", encoding="utf-8")
    route_pairs = (
        ("DBServer/AddrTable.txt", b"10.10.0.163\n"),
        ("DBServer/ServerInfo.txt", b"10.10.0.102 10.10.0.102 7200\n"),
        (
            "LoginSrv/AddrTable.txt",
            "热血传奇 Title1 10.10.0.102 10.10.0.102 10.10.0.102:7100\r\n".encode("gbk"),
        ),
        ("LoginSrv/ServerAddr.txt", b"10.10.0.163\r\n"),
        ("LoginSrv/UserLimit.txt", "热血传奇 热血传奇 10000\r\n".encode("gbk")),
    )
    for rel, fallback in route_pairs:
        src = mir_root / rel
        dst = fake / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(src.read_bytes() if src.is_file() else fallback)
    return fake


def _selftest_deploy(tmp: Path, repo_root: Path, mir_root: Path) -> int:
    fake = _selftest_synthetic_root(tmp, repo_root, mir_root)
    deploy_root = tmp / "deploy"
    result = deploy_stack(
        deploy_root, repo_root, fake, {}, db_mode="test", force=False, what_if=False
    )
    _print_mapping(result["confs"])
    for entry in result["copy"]["roles"]:
        print(
            f"DEPLOY {entry['role']}: {entry['files']} files "
            f"({'OK' if (deploy_root / Path(entry['output']).name).is_dir() else 'RED'})"
        )
    _print_routes(result["routes"])

    failures: list[str] = []
    # 1) 六份 conf + 数据库目标 + 生产库 fail-closed
    if len(result["confs"]["outputs"]) != 6:
        failures.append(f"conf 只生成 {len(result['confs']['outputs'])} 份")
    for role, prod_test in DB_TARGETS.items():
        path = Path(result["confs"]["outputs"][role]["output"])
        text = path.read_bytes()
        if f"database={prod_test[1]};".encode() not in text:
            failures.append(f"{role}: 缺 database={prod_test[1]};")
        try:
            assert_no_prod_db(text, path)
        except ProfileError as exc:
            failures.append(str(exc))
    # 2) 路由文件内容断言
    checks = {
        "LoginSrv/AddrTable.txt": [
            (b"127.0.0.1 127.0.0.1", "Remote/Public"),
            (b"127.0.0.1:17100", "SelGate"),
            ("热血传奇".encode("gbk"), "服务器名"),
        ],
        "DBSrv/ServerInfo.txt": [
            (b"127.0.0.1 127.0.0.1 17200", "SelGate->GameGate"),
        ],
        "LoginSrv/ServerAddr.txt": [(b"127.0.0.1", "IDS whitelist")],
        "DBSrv/AddrTable.txt": [(b"127.0.0.1", "DB whitelist")],
        "LoginSrv/UserLimit.txt": [("热血传奇".encode("gbk"), "UserLimit 名")],
    }
    for rel, needles in checks.items():
        payload = (deploy_root / rel).read_bytes()
        for needle, label in needles:
            if needle not in payload:
                failures.append(f"{rel}: 缺 {label} {needle!r}")
        print(f"ROUTE_ASSERT {rel}: {len(needles)} 条 OK")
    # 3) 非空根无 --force 必须拒绝
    try:
        deploy_stack(
            deploy_root, repo_root, fake, {}, db_mode="test", force=False, what_if=False
        )
        failures.append("非空 deploy 根未加 --force 却通过了")
    except ProfileError as exc:
        print(f"DEPLOY_GUARD_OK 非空根拒绝覆盖：{exc}")
    # 4) 生成根的文件数 > 配置数（证明角色树被复制）
    marker = deploy_root / "M2GameSvr" / "marker.txt"
    if not marker.is_file():
        failures.append("角色树未复制（缺 marker.txt）")
    if failures:
        for msg in failures:
            print(f"RED: {msg}", file=sys.stderr)
        return 1
    return 0


def cmd_selftest(args: argparse.Namespace) -> int:
    repo_root = Path(args.repo_root).resolve()
    mir_root = Path(args.mir_server_root)

    rt = sp.roundtrip_check()
    if not (rt["plaintext_ok"] and rt["base64_ok"]):
        print(f"RED: 固定向量回落 {rt}", file=sys.stderr)
        return 1
    print(
        f"ROUNDTRIP_OK plaintext=656 base64_len={rt['base64_len']} "
        f"iv={rt['iv_hex']} addr={rt['server_addr']} port={rt['server_port']}"
    )

    with tempfile.TemporaryDirectory(prefix="altport-selftest-") as tmp:
        tmp_path = Path(tmp)
        rc = _selftest_deploy(tmp_path, repo_root, mir_root)
        if rc != 0:
            return rc

        client_dir = DEFAULT_CLIENT_RUN_DIR
        if (client_dir / "run-release.cmd").is_file():
            out_client = tmp_path / "client"
            out_client.mkdir()
            src_bytes = (client_dir / "run-release.cmd").read_bytes()
            (out_client / "run-release.cmd").write_bytes(src_bytes)
            client = generate_client_cmd(
                out_client, port=DEFAULT_CLIENT_PORT, addr=DEFAULT_CLIENT_ADDR
            )
            if (client_dir / "run-release.cmd").read_bytes() != src_bytes:
                print("RED: 原 run-release.cmd 被改动", file=sys.stderr)
                return 1
            print(
                f"CLIENT_OK decrypt_addr={client['verify']['decrypted_addr']} "
                f"decrypt_port={client['verify']['decrypted_port']} "
                f"param_len={client['verify']['param_len']}"
            )
        else:
            print(f"CLIENT_SKIP 未找到 {client_dir / 'run-release.cmd'}")

    print("ALT_PORT_SELFTEST_OK")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="备用端口剖面：生成六份影子 conf（15xxx/17xxx）+ 可选整套部署根 + 客户端启动命令"
    )
    sub = parser.add_subparsers(dest="cmd", required=True)

    def common(p: argparse.ArgumentParser) -> None:
        p.add_argument(
            "-MirServerRoot", "--mir-server-root", "--mirserver",
            default=str(DEFAULT_MIR_SERVER_ROOT),
        )
        p.add_argument(
            "-RepoRoot",
            "--repo-root",
            default=str(Path(__file__).resolve().parents[2]),
            help="仓库根（用于 src/DBSrv|LoginSrv 的 Rust 口径配置）",
        )
        p.add_argument(
            "--source", action="append", default=[], metavar="ROLE=PATH",
            help="覆盖某角色的源配置（可重复）",
        )

    p_gen = sub.add_parser("generate", help="生成影子 conf / 完整部署根（+ 可选客户端命令）")
    common(p_gen)
    p_gen.add_argument(
        "-OutDir", "--out-dir", default="",
        help="覆盖层输出根（只写六份 conf；不是可独立起栈的 -Root）",
    )
    p_gen.add_argument(
        "--deploy", default="", metavar="DIR",
        help="完整部署根：复制 -MirServerRoot 的六个角色树（源只读）+ 改写路由文件",
    )
    p_gen.add_argument(
        "--db-mode", choices=("test", "prod"), default="test",
        help="test=连接串改到 mir2_test_*（默认，窗口用）；prod=保留源连接串",
    )
    p_gen.add_argument(
        "-ClientRunDir", "--client-run-dir", default="",
        help="客户端目录；只写 run-release-alt.cmd（只读源写后哈希断言）",
    )
    p_gen.add_argument("--client-port", type=int, default=DEFAULT_CLIENT_PORT)
    p_gen.add_argument("--client-addr", default=DEFAULT_CLIENT_ADDR)
    p_gen.add_argument("--iv", choices=("preserve", "random"), default="preserve")
    p_gen.add_argument(
        "--force", action="store_true",
        help="允许覆盖已存在的 run-release-alt.cmd / 非空 --deploy 根",
    )
    p_gen.add_argument("-WhatIf", "--what-if", dest="what_if", action="store_true",
                       help="只打印映射，不写任何文件")
    p_gen.add_argument("--json", action="store_true")
    p_gen.set_defaults(func=cmd_generate)

    p_map = sub.add_parser("mapping", help="打印固定剖面与计划键（只读）")
    common(p_map)
    p_map.set_defaults(func=cmd_mapping)

    p_st = sub.add_parser("selftest", help="roundtrip + 部署/路由/连接串/客户端离线自检")
    common(p_st)
    p_st.set_defaults(func=cmd_selftest)

    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        return args.func(args)
    except ProfileError as exc:
        print(f"RED: {exc}", file=sys.stderr)
        return 1
    except sp.StartupParamError as exc:
        print(f"RED: {exc}", file=sys.stderr)
        return 1
    except (OSError, ValueError) as exc:
        print(f"RED: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
