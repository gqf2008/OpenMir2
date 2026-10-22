#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""备用端口剖面（15xxx / 17xxx）：生成六份影子 conf + 一个客户端启动命令。

为什么要有这个工具
------------------
真机窗口现在必须占用 5000 / 7000 / 7200（以及 5100 / 5500 / 5600 / 6000 / 7100），
与外部 C# 全栈直接冲突。本工具把整套链路的端口**整体平移到备用段**，生成六份
**影子配置**（源文件只读、不覆盖）与一个新的客户端启动命令，后续窗口用影子配置
起栈即可，不再抢原端口。

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
    （与 SelGate 的 `LsvServerPort=16005` 对齐；Rust/C# 当前都未消费它，纯保留）。
  - `bins/game-svr/src/config.rs`：[Server] GatePort=监听(5000)、DBPort=db-srv DataService(6000)、
    IDSPort=login-srv IDS(5600)。
  - `bins/login-gate/src/config.rs`：[LoginGate] ServerPort0=LoginSrv(5500)、GatePort0=客户端(7000)。
  - `bins/sel-gate/src/config.rs`：[SelGate] ServerPort0=DBSrv(5100)、GatePort0=客户端(7100)；
    `LsvServerPort` 未被解析器读取（保持 16005 不动）。
  - `bins/game-gate/src/config.rs`：[GameGate] Count 组 ServerPortN=GameSvr(5000)、GatePortN=客户端(7200..)。

编码口径
--------
六个源文件里既有 UTF-8 BOM（`dbsvr.conf` / `Server.conf`），也有含非 UTF-8 字节的序列
（`RunGate/config.conf` 的 `[Strings]` 段实测有 `EF BC 3F`；既不是合法 UTF-8 也不是合法 GBK）。
本工具按 **latin-1 逐字节**读写（等价 `tools/m2-acceptance/db_target_plumb.ps1` 的做法），
只替换目标键的值 token：BOM、非 UTF-8 字节、CRLF、段序、注释全部 1:1 保留。
写完后用「差异行集合 == 计划键集合」做 fail-closed 校验。

安全边界
--------
不启动任何进程、不占端口、不写数据库、不写 `E:\\MirServer`；源文件只读。
客户端部分只读 `run-release.cmd`，只新增 `run-release-alt.cmd`。
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
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

# 固定剖面（用于文档/`mapping` 子命令；实际改写值见 ROLE_SPECS）。
PROFILE: dict[str, str] = {
    "game-svr": "15000（GatePort；GameGate 下游）",
    "db-srv": "16000（ServerPort）/ 15100（GatePort）/ 15700（MarketPort）",
    "login-srv": "15500（GatePort）/ 15600（ServerPort）/ 16005（RunPort）",
    "login-gate": "17000（GatePort0；客户端入口）",
    "sel-gate": "17100（GatePort0；客户端入口）",
    "game-gate": "17200 / 17201 / 17202（GatePortN；保留 Count=3 间距）",
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
) -> dict[str, Any]:
    outputs: dict[str, Any] = {}
    port_map: list[dict[str, Any]] = []
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

        out = out_dir / spec.deploy_dir / spec.filename
        if not what_if:
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_bytes(new_data)
            _verify_output(new_data, edits, out)

        for c in changed:
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
            "diff_set": sorted(f"[{c['section']}] {c['key']}" for c in changed),
        }
    return {"outputs": outputs, "port_map": port_map}


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


def cmd_generate(args: argparse.Namespace) -> int:
    repo_root = Path(args.repo_root).resolve()
    mir_root = Path(args.mir_server_root)
    out_dir = Path(args.out_dir)
    overrides = _parse_overrides(args.source)

    result = generate_confs(
        out_dir, repo_root, mir_root, overrides, what_if=args.what_if
    )
    _print_mapping(result)

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
        "mir_server_root": str(mir_root),
        "repo_root": str(repo_root),
        "out_dir": str(out_dir),
        "port_profile": PROFILE,
        "outputs": result["outputs"],
        "port_map": result["port_map"],
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
        result = generate_confs(tmp_path / "root", repo_root, mir_root, {})
        _print_mapping(result)
        if len(result["outputs"]) != 6:
            print(f"RED: 只生成 {len(result['outputs'])} 份 conf", file=sys.stderr)
            return 1
        if not result["port_map"]:
            print("RED: port_map 为空", file=sys.stderr)
            return 1

        # 逐份把输出读回，复核目标键值 + 差异集合。
        for spec in ROLE_SPECS:
            info = result["outputs"][spec.role]
            out = Path(info["output"])
            data = out.read_bytes()
            source = Path(info["source"]).read_bytes()
            edits = spec.planned_edits(source)
            _verify_output(data, edits, out)
            if len(_lines(data)) != len(_lines(source)):
                print(f"RED: {spec.role} 行数变化", file=sys.stderr)
                return 1

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
                f"decrypt_port={client['verify']['decrypted_port']}"
            )
        else:
            print(f"CLIENT_SKIP 未找到 {client_dir / 'run-release.cmd'}")

    print("ALT_PORT_SELFTEST_OK")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="备用端口剖面：生成六份影子 conf（15xxx/17xxx）+ 客户端启动命令"
    )
    sub = parser.add_subparsers(dest="cmd", required=True)

    def common(p: argparse.ArgumentParser) -> None:
        p.add_argument(
            "-MirServerRoot", "--mir-server-root", default=str(DEFAULT_MIR_SERVER_ROOT)
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

    p_gen = sub.add_parser("generate", help="生成影子 conf（+ 可选客户端命令）")
    common(p_gen)
    p_gen.add_argument(
        "-OutDir", "--out-dir", required=True,
        help="输出根（布局与 start_mir2_server.ps1 -Root 一致）",
    )
    p_gen.add_argument(
        "-ClientRunDir", "--client-run-dir", default="",
        help="客户端目录；只写 run-release-alt.cmd",
    )
    p_gen.add_argument("--client-port", type=int, default=DEFAULT_CLIENT_PORT)
    p_gen.add_argument("--client-addr", default=DEFAULT_CLIENT_ADDR)
    p_gen.add_argument("--iv", choices=("preserve", "random"), default="preserve")
    p_gen.add_argument("--force", action="store_true", help="允许覆盖已存在的 run-release-alt.cmd")
    p_gen.add_argument("-WhatIf", "--what-if", dest="what_if", action="store_true",
                       help="只打印映射，不写任何文件")
    p_gen.add_argument("--json", action="store_true")
    p_gen.set_defaults(func=cmd_generate)

    p_map = sub.add_parser("mapping", help="打印固定剖面与计划键（只读）")
    common(p_map)
    p_map.set_defaults(func=cmd_mapping)

    p_st = sub.add_parser("selftest", help="roundtrip + 生成/差异/客户端离线自检")
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
