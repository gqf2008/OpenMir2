#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""OpenMir2 客户端启动参数的离线读写工具（base64(IV8 + Blowfish-CBC(656B))）。

算法来源（只读引用，未修改对端仓）：
  E:/Users/gxh/Documents/GitHub/MirClientZig/src/app/boot.zig:280-360
  - password = 两段 UUID 直接拼接（见 PASSWORD）；
  - key      = sha1(password.encode("utf-16-le")).digest()   # 20 字节；
  - 参数串   = base64(raw[:8] + Blowfish-CBC(key, iv=raw[:8]).encrypt(plaintext))；
  - 明文 656 字节，字段布局见 SHORT_FIELDS / I32_FIELDS。

固定向量（同 boot.zig）：
  ORACLE_BASE64 解密后必须**逐字节**等于 FIXED_PLAINTEXT_HEX；
  再用 raw[:8] 作 IV 加密回去必须得到**同一个** base64 串。

本工具不启动客户端、不联网、不写数据库，只做字节级读写。
"""

from __future__ import annotations

import argparse
import base64
import binascii
import hashlib
import json
import sys
from typing import Any

PLAINTEXT_LEN = 656
IV_LEN = 8

# boot.zig:294-296 的两段 UUID（直接拼接，中间保留连字符）。
PASSWORD = (
    "E3894C0D-F0DA-4DA6-828B-4E17FAB36B87-"
    "5A96ADC0-E444-4D5F-A3B7-8D424727BC56"
)

# boot.zig:304-318 `fixed_plaintext_hex` 的 656 字节明文（按 boot.zig 的偏移布局）。
FIXED_PLAINTEXT_HEX = (
    "0c4c6567656e644f664d697232000000000000000000000000000000000000093132372e302e302e3100000000000000"
    "000000000000000000000000000020366146756f6554725a5a4d766a71756c4e7a7a46677365654c61504c6c434e3200"
    "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    "00000000095265736f757263655c00000000000000000000000000000000000000000000000000000000000000000000"
    "0000000000000000581b00000001000100040000000300003b2900000100000000000000000000000000000000000000"
    "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    "0000000000000000000000000000000000000000000000000000000000000000"
)

# boot.zig:324-325 `oracle_base64` 原值（888 字符）。
ORACLE_BASE64 = (
    "OLzYD5oEDvf+iBkRdWfjjq2Vp9LVU1K8WIeuAaqgIxIcVO5wbDaXGSX5PjYT3jqkkQWeGVeU6z8rHjfl5P3xuk6ya+SDlXpM"
    "8f/IBhle42pmcp+Td4Ges5A3N7kFAs3wHVwKv+ZMLeI6H2TOpgudNtMrQvWMSHE5Wte/HU+urDYJbHdY4hi12imA6NWIJmRo"
    "r0/7KKRkTSCC+s0aBh9/UJ2gw39g9Cev4DsU8u+D5tygyt+Qhki1dmbxY0rt2Kf29gjMS/iuiUaGziHD+fPptq+WuO7HtoAr"
    "uAIA9eToirLGuGe7DiSLeIWY8+yZCwZuWVzaFxYPRDaVHSSsftdyf3hl8Sati4kRjXBUJY+zM7W2FOHQpuBwoTI6RUV5fmM/"
    "iPzVWOTVnB1SmmpeC5FWKCEA5EJHgBma5E4Xfb6uxubTw7vRyxAaZdtpUJoSrZ/JMB7ZpWonYH8wL4IX3gA+tmeRH/zQ0t/l"
    "rM0PRPvoqMJKL/NdCigFChWz3Cshm8z/VRkKZaH0FP2PuUiZvSO6jI19iz53EyHnbyJBrUCorfFqFHzTXKWh2Ax4+qmgZNU9"
    "2JtqhIRjQS4uVujOC1Xf3TnFkOpJYqpCX531xabeQNU5aKFNWgfnrW2M2/36hTeddHX27Td9N81qlEXnyP28PCqGUr9XKAjz"
    "ivsjWRfhgwBpVP5omT4BuzRedc5geOuFhQea5hBLb36jNO/zVhVWll1bKOhXlT4szF4x3f4FcmMvzHxdeM1LgqZIInyPVmrI"
    "Scu5GY0e1s6Xo2UNTSc2Qw6cgPsTzBmzAVv1vNAiVurxjlnJHLi1VtBpx4bLWa3Xep+aMTLAotkpf0Nq4Zts9IoxWi51AKDH"
    "N78Qx3ea58s0tSo7VnEbBA=="
)

# (字段名, 偏移, 容量)。短串 = 1 字节长度 + 容量字节；容量含末尾 0。
SHORT_FIELDS: tuple[tuple[str, int, int], ...] = (
    ("server_name", 0, 30),
    ("server_addr", 31, 30),
    ("server_key", 62, 100),
    ("uipak_key", 163, 32),
    ("resource_dir", 196, 50),
    ("logo", 269, 255),
    ("password_file", 525, 127),
)

# (字段名, 偏移)。小端 i32。
I32_FIELDS: tuple[tuple[str, int], ...] = (
    ("server_port", 248),
    ("screen_width", 256),
    ("screen_height", 260),
    ("local_mini_port", 264),
)

# (字段名, 偏移)。单字节。
U8_FIELDS: tuple[tuple[str, int], ...] = (
    ("full_screen", 252),
    ("wait_vblank", 253),
    ("mode3d", 254),
    ("mini", 255),
    ("client_ver", 268),
)

PORT_OFFSET = 248
ADDR_OFFSET, ADDR_CAPACITY = 31, 30


class StartupParamError(ValueError):
    """启动参数格式/长度/口令/加密库不可用。"""


def _key() -> bytes:
    return hashlib.sha1(PASSWORD.encode("utf-16-le")).digest()


def _blowfish_cipher(iv: bytes, key: bytes):
    """构造 Blowfish-CBC 的 Cipher；只依赖 cryptography（已探测 50.0.1 可用）。"""
    if len(iv) != IV_LEN:
        raise StartupParamError(f"IV 必须 {IV_LEN} 字节，实际 {len(iv)}")
    try:  # cryptography >= 43：Blowfish 在 decrepit 命名空间。
        from cryptography.hazmat.decrepit.ciphers.algorithms import Blowfish
    except ImportError:  # 旧版本仍从 primitives 导出。
        from cryptography.hazmat.primitives.ciphers.algorithms import Blowfish
    from cryptography.hazmat.primitives.ciphers import Cipher, modes

    return Cipher(Blowfish(key), modes.CBC(iv))


def _clean_token(token: str) -> str:
    token = token.strip()
    if len(token) >= 2 and token[0] == token[-1] and token[0] in "\"'":
        token = token[1:-1]
    return token


def decrypt(param_b64: str) -> bytes:
    """base64 参数 → 656 字节明文。长度/Base64 非法时抛 StartupParamError。"""
    token = _clean_token(param_b64)
    if not token:
        raise StartupParamError("启动参数为空")
    try:
        raw = base64.b64decode(token, validate=True)
    except (binascii.Error, ValueError) as exc:
        raise StartupParamError(f"启动参数不是合法 Base64：{exc}") from exc
    if len(raw) != IV_LEN + PLAINTEXT_LEN:
        raise StartupParamError(
            f"启动参数解码后 {len(raw)} 字节，期望 {IV_LEN + PLAINTEXT_LEN}"
            "（8 字节 IV + 656 字节明文）"
        )
    dec = _blowfish_cipher(raw[:IV_LEN], _key()).decryptor()
    return dec.update(raw[IV_LEN:]) + dec.finalize()


def encrypt(plaintext: bytes, iv: bytes) -> str:
    """656 字节明文 + 8 字节 IV → base64（IV 前缀 + CBC 密文）。"""
    if len(plaintext) != PLAINTEXT_LEN:
        raise StartupParamError(
            f"明文必须 {PLAINTEXT_LEN} 字节，实际 {len(plaintext)}"
        )
    if len(iv) != IV_LEN:
        raise StartupParamError(f"IV 必须 {IV_LEN} 字节，实际 {len(iv)}")
    enc = _blowfish_cipher(iv, _key()).encryptor()
    return base64.b64encode(iv + enc.update(plaintext) + enc.finalize()).decode("ascii")


def read_short(buf: bytes, offset: int, capacity: int) -> bytes:
    """读取 Delphi 短串：1 字节长度 + 内容；越界/超容量报错。"""
    if offset >= len(buf):
        raise StartupParamError(f"短串偏移 {offset} 越界（len={len(buf)}）")
    n = buf[offset]
    if n > capacity:
        raise StartupParamError(
            f"短串长度 {n} 超过容量 {capacity}（偏移 {offset}）"
        )
    end = offset + 1 + n
    if end > len(buf):
        raise StartupParamError(f"短串数据越界：{offset}..{end}（len={len(buf)}）")
    return bytes(buf[offset + 1 : end])


def write_short(buf: bytearray, offset: int, capacity: int, value: bytes) -> None:
    """写 Delphi 短串：整槽先清零（等价 .NET 定长数组），再写长度 + 内容。"""
    if len(value) > capacity:
        raise StartupParamError(
            f"短串值 {len(value)} 字节超过容量 {capacity}：{value!r}"
        )
    end = offset + 1 + capacity
    if end > len(buf):
        raise StartupParamError(f"短串槽越界：{offset}..{end}（len={len(buf)}）")
    buf[offset:end] = b"\x00" * (1 + capacity)
    buf[offset] = len(value)
    buf[offset + 1 : offset + 1 + len(value)] = value


def _display(raw: bytes) -> str:
    for enc in ("utf-8", "gbk"):
        try:
            return raw.decode(enc)
        except UnicodeDecodeError:
            continue
    return raw.decode("latin-1")


def parse(plaintext: bytes) -> dict[str, Any]:
    """把 656 字节明文解析成可读字段（长度/容量非法时报错）。"""
    if len(plaintext) != PLAINTEXT_LEN:
        raise StartupParamError(
            f"明文必须 {PLAINTEXT_LEN} 字节，实际 {len(plaintext)}"
        )
    buf = bytes(plaintext)
    out: dict[str, Any] = {}
    for name, offset, capacity in SHORT_FIELDS:
        raw = read_short(buf, offset, capacity)
        out[name] = _display(raw)
        out[name + "_hex"] = raw.hex()
    for name, offset in I32_FIELDS:
        out[name] = int.from_bytes(buf[offset : offset + 4], "little", signed=True)
    for name, offset in U8_FIELDS:
        out[name] = buf[offset]
    return out


def patch_port(plaintext: bytes, port: int) -> bytes:
    """只改 [248,252) 的 i32 端口，其余字节不动。"""
    if not 1 <= int(port) <= 65535:
        raise StartupParamError(f"端口必须在 1..65535：{port}")
    if len(plaintext) != PLAINTEXT_LEN:
        raise StartupParamError(f"明文必须 {PLAINTEXT_LEN} 字节")
    buf = bytearray(plaintext)
    buf[PORT_OFFSET : PORT_OFFSET + 4] = int(port).to_bytes(4, "little", signed=True)
    return bytes(buf)


def patch_addr(plaintext: bytes, host: str) -> bytes:
    """只改组 31 的 30 字节短串（GBK 编码），其余字节不动。"""
    try:
        value = host.encode("gbk")
    except UnicodeEncodeError as exc:
        raise StartupParamError(f"地址无法按 GBK 编码：{host!r}（{exc}）") from exc
    buf = bytearray(plaintext)
    write_short(buf, ADDR_OFFSET, ADDR_CAPACITY, value)
    return bytes(buf)


def fixed_plaintext() -> bytes:
    return bytes.fromhex(FIXED_PLAINTEXT_HEX)


def roundtrip_check() -> dict[str, Any]:
    """固定向量判据：解密逐字节 + 以原 IV 加密回同一串。"""
    plain = decrypt(ORACLE_BASE64)
    expected = fixed_plaintext()
    plaintext_ok = plain == expected
    raw = base64.b64decode(ORACLE_BASE64, validate=True)
    again = encrypt(plain, raw[:IV_LEN])
    base64_ok = again == ORACLE_BASE64
    info = parse(plain) if plaintext_ok else {}
    return {
        "plaintext_ok": plaintext_ok,
        "base64_ok": base64_ok,
        "plaintext_len": len(plain),
        "base64_len": len(ORACLE_BASE64),
        "iv_hex": raw[:IV_LEN].hex(),
        "server_addr": info.get("server_addr"),
        "server_port": info.get("server_port"),
    }


def _read_param_arg(args: argparse.Namespace) -> str:
    if args.param == "-":
        return sys.stdin.read()
    if args.param:
        return args.param
    if args.param_file:
        import re

        text = open(args.param_file, "r", encoding="latin-1").read()
        m = re.search(r"[\"']([A-Za-z0-9+/=]{64,})[\"']", text)
        if not m:
            m = re.search(r"\b([A-Za-z0-9+/=]{64,})\b", text)
        if not m:
            raise StartupParamError(f"文件里找不到 Base64 启动参数：{args.param_file}")
        return m.group(1)
    raise StartupParamError("请给 base64 参数、--param-file 或 -（stdin）")


def _print_json(obj: Any) -> None:
    print(json.dumps(obj, ensure_ascii=False, indent=2, sort_keys=True))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="OpenMir2 客户端启动参数（656B 明文 / Blowfish-CBC）离线工具"
    )
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_dec = sub.add_parser("decrypt", help="解密并打印字段")
    p_dec.add_argument("param", nargs="?", help="base64 参数（或用 - 读 stdin）")
    p_dec.add_argument("--param-file", help="从 cmd/文本文件提取 base64 参数")
    p_dec.add_argument("--plaintext-out", help="把 656B 明文写到该文件")
    p_dec.add_argument("--json", action="store_true", help="输出 JSON")

    p_enc = sub.add_parser("encrypt", help="加密 656B 明文")
    p_enc.add_argument("--plaintext", required=True, help="656B 明文文件")
    p_enc.add_argument("--iv", required=True, help="8B IV 的 hex（16 个十六进制字符）")
    p_enc.add_argument("--out", help="把 base64 写到此文件（默认 stdout）")

    p_port = sub.add_parser("port", help="读取/改写明文端口")
    p_port.add_argument("param", nargs="?", help="base64 参数（或用 --param-file）")
    p_port.add_argument("--param-file", help="从 cmd/文本文件提取 base64 参数")
    p_port.add_argument("--set", type=int, metavar="PORT", help="改成新端口并输出新 base64")

    sub.add_parser("roundtrip", help="固定向量自检")
    sub.add_parser("selftest", help="roundtrip 的别名")

    args = parser.parse_args(argv)

    try:
        if args.cmd in ("roundtrip", "selftest"):
            r = roundtrip_check()
            print(f"PLAINTEXT_OK={str(r['plaintext_ok']).lower()} bytes={r['plaintext_len']}")
            print(f"ROUNDTRIP_OK={str(r['base64_ok']).lower()} base64_len={r['base64_len']}")
            print(f"IV_HEX={r['iv_hex']} ADDR={r['server_addr']} PORT={r['server_port']}")
            return 0 if (r["plaintext_ok"] and r["base64_ok"]) else 1

        if args.cmd == "decrypt":
            token = _read_param_arg(args)
            plain = decrypt(token)
            if args.plaintext_out:
                with open(args.plaintext_out, "wb") as fh:
                    fh.write(plain)
            info = parse(plain)
            if args.json:
                _print_json(info)
            else:
                for k in (
                    "server_name",
                    "server_addr",
                    "server_key",
                    "uipak_key",
                    "resource_dir",
                    "server_port",
                    "full_screen",
                    "wait_vblank",
                    "mode3d",
                    "mini",
                    "screen_width",
                    "screen_height",
                    "local_mini_port",
                    "client_ver",
                    "logo",
                    "password_file",
                ):
                    print(f"{k}={info[k]}")
            return 0

        if args.cmd == "encrypt":
            plain = open(args.plaintext, "rb").read()
            iv = bytes.fromhex(args.iv)
            token = encrypt(plain, iv)
            if args.out:
                with open(args.out, "w", encoding="ascii", newline="") as fh:
                    fh.write(token)
            else:
                print(token)
            return 0

        if args.cmd == "port":
            token = _read_param_arg(args)
            plain = decrypt(token)
            old = int.from_bytes(plain[PORT_OFFSET : PORT_OFFSET + 4], "little", signed=True)
            if args.set is None:
                print(f"server_addr={_display(read_short(plain, ADDR_OFFSET, ADDR_CAPACITY))}")
                print(f"server_port={old}")
                return 0
            new_plain = patch_port(plain, args.set)
            raw = base64.b64decode(_clean_token(token), validate=True)
            print(encrypt(new_plain, raw[:IV_LEN]))
            print(f"# server_port {old} -> {args.set}", file=sys.stderr)
            return 0

        parser.error("未知子命令")
        return 2
    except StartupParamError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    except (OSError, ValueError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
