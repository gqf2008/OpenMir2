#!/usr/bin/env python3
"""C# LoginSrv 金标准采集 —— 用「假 LoginGate」直连真身，逐条记录响应字节。

用途（A2/M1，Rust 版 LoginSrv）：把 C# `src/LoginSrv/**` 的真实响应**逐字节**钉住，
供 `bins/login-srv` 的进程级自测做字面量断言（自洽往返对格式改动是盲的，
见 `RULE_规则可执行性.md`）。

链路（与 Rust `bins/login-gate` 完全同形）：
    6B `ServerDataPacket`([PacketCode u32=0xAA9AAA9A][PacketLen u16]) + MemoryPack 体
    `ServerDataMessage` = [0x04][type u8][socket_id: i32 UTF-16 码元数 + UTF-16LE]
                           [DataLen i16][data: i32 长度 + 字节]
    type: Enter=0 / Leave=1 / Data=2 / KeepAlive=3
`Data` 的载荷就是客户端原始帧 `#1<EDCode(12B 头)><EDCode(体)>!`（网关原样透传）。

用法：
    python oracle_capture.py --src <含 LoginSrv.exe 的目录> --out <out.json> [--port 5599]

依赖：只读 `--src`，全部写入临时目录（不碰 E:\\MirServer、不碰仓库），
启动前需本机 MySQL 3306 与 `mir2_account` 可用（脚本只读账号，登录会写
`PassFailCount/LastLoginTime`，用到的 `--account` 需真实存在）。
"""

import argparse
import json
import os
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time

SEED = 0xAC
BASE = 0x3C
PACKET_CODE = 0xAA9AAA9A

# ---------------------------------------------------------------- EDCode (C# EncryptUtil)


def edcode_encode(src: bytes) -> bytes:
    out = bytearray()
    no = 2
    rem = 0
    for b in src:
        c = b ^ SEED
        if no == 6:
            out.append(((c & 0x3F) + BASE) & 0xFF)
            rem |= (c >> 2) & 0x30
            out.append((rem + BASE) & 0xFF)
            rem = 0
        else:
            temp = c >> 2
            out.append((((temp & 0x3C) | (c & 3)) + BASE) & 0xFF)
            rem = ((rem << 2) | (temp & 3)) & 0xFF
        no = no % 6 + 2
    if no != 2:
        out.append((rem + BASE) & 0xFF)
    return bytes(out)


def edcode_decode(src: bytes) -> bytes:
    out = bytearray()
    n = len(src)
    cyc, left = n // 4, n % 4
    m = lambda x: (x - BASE) & 0xFF  # noqa: E731
    for i in range(cyc):
        b = i * 4
        r = m(src[b + 3])
        t = m(src[b])
        out.append((((t << 2) & 0xF0) | (r & 0x0C) | (t & 3)) ^ SEED)
        t = m(src[b + 1])
        out.append((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 3)) ^ SEED)
        t = m(src[b + 2])
        out.append(((t | ((r << 2) & 0xC0))) ^ SEED)
    if left == 2:
        r = m(src[n - 1])
        t = m(src[n - 2])
        out.append((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 3)) ^ SEED)
    elif left == 3:
        r = m(src[n - 1])
        t = m(src[n - 3])
        out.append((((t << 2) & 0xF0) | (r & 0x0C) | (t & 3)) ^ SEED)
        t = m(src[n - 2])
        out.append((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 3)) ^ SEED)
    return bytes(out)


def cmd_head(ident: int, recog: int = 0, param: int = 0, tag: int = 0, series: int = 0) -> bytes:
    return struct.pack("<iHHHH", recog, ident, param, tag, series)


def client_frame(ident: int, body: bytes = b"", **kw) -> bytes:
    """客户端帧 `#1<enc(头)>[<enc(体)>]!`（头体分别编码后拼接）。"""
    out = b"#1" + edcode_encode(cmd_head(ident, **kw))
    if body:
        out += edcode_encode(body)
    return out + b"!"


# ---------------------------------------------------------------- 内部链路帧


def sdm(kind: int, socket_id: str, data: bytes, data_len=None) -> bytes:
    units = socket_id.encode("utf-16-le")
    n_units = len(units) // 2
    if data_len is None:
        data_len = len(data)
    body = bytes([0x04, kind])
    body += struct.pack("<i", n_units) + units
    body += struct.pack("<h", data_len)
    body += struct.pack("<i", len(data)) + data
    return struct.pack("<IH", PACKET_CODE, len(body)) + body


def read_frames(sock: socket.socket, timeout: float = 2.5):
    """读一坨内部帧，返回 (frames, leftovers)。"""
    frames = []
    buf = b""
    sock.settimeout(timeout)
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            chunk = sock.recv(65536)
        except socket.timeout:
            break
        if not chunk:
            break
        buf += chunk
        while len(buf) >= 6:
            code, plen = struct.unpack_from("<IH", buf, 0)
            if code != PACKET_CODE:
                buf = buf[1:]
                continue
            if len(buf) < 6 + plen:
                break
            body = buf[6 : 6 + plen]
            frames.append(body)
            buf = buf[6 + plen :]
    return frames, buf


def parse_sdm(body: bytes):
    """解析 ServerDataMessage MemoryPack 体。"""
    assert body[0] == 0x04, f"object header {body[0]:#x}"
    kind = body[1]
    pos = 2
    (n_units,) = struct.unpack_from("<i", body, pos)
    pos += 4
    sid = body[pos : pos + n_units * 2].decode("utf-16-le")
    pos += n_units * 2
    (data_len,) = struct.unpack_from("<h", body, pos)
    pos += 2
    (n_data,) = struct.unpack_from("<i", body, pos)
    pos += 4
    data = body[pos : pos + n_data]
    return {"type": kind, "socket_id": sid, "data_len": data_len, "data": data}


def split_client_frame(raw: bytes):
    """把 s2c 帧拆成 (帧尾, 段列表)：`#` + 若干「各自独立编码」的段 + 帧尾。

    段边界按 C# 发送点给定（调用方给 seg_plain_lens），这里只做「已知布局」的校验性拆分；
    未知布局按单段整体解码（`body_layout=flat`）。
    """
    if raw[-2:] == b"!$":
        tail = b"!$"
    elif raw[-1:] == b"!":
        tail = b"!"
    else:
        raise ValueError(f"bad tail {raw[-4:]!r}")
    inner = raw[1 : len(raw) - len(tail)]
    head_enc_len = 16
    head = edcode_decode(inner[:head_enc_len])
    rest = inner[head_enc_len:]
    return tail, head, rest


def head_fields(head: bytes):
    recog, ident, param, tag, series = struct.unpack("<iHHHH", head)
    return {
        "ident": ident,
        "recog": recog,
        "param": param,
        "tag": tag,
        "series": series,
    }


# ---------------------------------------------------------------- 采集主体


def write_workdir(work: str, gate_port: int, srv_port: int, dsn: str, server_name: str):
    conf = f"""[Server]
ReadyServers=0
EnableMakingID=1
EnableTrial=FALSE
TestServer=1
GateAddr=127.0.0.1
GatePort={gate_port}
ServerAddr=127.0.0.1
ServerPort={srv_port}
MonAddr=127.0.0.1
MonPort=3000
DynamicIPMode=0
EnableGetbackPassword=1
DBServer=127.0.0.1
DBSPort=6000
FeeServer=127.0.0.1
LogServer=127.0.0.1
RunAddr=127.0.0.1
RunPort=7200
UnLockAccountTick=60
LoginNo=99999

[DataBase]
DbType=MySQL
ConnctionString={dsn}
"""
    with open(os.path.join(work, "logsrv.conf"), "w", encoding="ascii") as f:
        f.write(conf)
    # AddrTable / UserLimit / ServerAddr 是 GB2312 文件（服务名是中文）
    with open(os.path.join(work, "AddrTable.txt"), "w", encoding="gbk") as f:
        f.write(f"{server_name} Title1 127.0.0.1 127.0.0.1 127.0.0.1:7100\n")
    with open(os.path.join(work, "ServerAddr.txt"), "w", encoding="ascii") as f:
        f.write("127.0.0.1\n")
    with open(os.path.join(work, "UserLimit.txt"), "w", encoding="gbk") as f:
        f.write(f"{server_name} {server_name} 10000\n")


def wait_port(port: int, timeout: float = 20.0) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), 0.5):
                return True
        except OSError:
            time.sleep(0.25)
    return False


def session(port, sid, calls):
    """一次 gate 会话：Enter → 若干 Data → （leave）。calls = [(name, frame_bytes)]。"""
    s = socket.create_connection(("127.0.0.1", port), 5)
    s.sendall(sdm(0, sid, b"127.0.0.1/127.0.0.1"))
    out = []
    for name, frame, segs in calls:
        s.sendall(sdm(2, sid, frame, data_len=len(frame)))
        bodies, leftover = read_frames(s)
        recs = []
        for b in bodies:
            m = parse_sdm(b)
            if m["type"] != 2:
                recs.append({"type": m["type"], "socket_id": m["socket_id"]})
                continue
            raw = m["data"]
            tail, head, rest = split_client_frame(raw)
            rec = {
                "request": name,
                "sdm_type": m["type"],
                "sdm_socket_id": m["socket_id"],
                "sdm_data_len": m["data_len"],
                "raw_hex": raw.hex(),
                "tail": tail.decode(),
                "ident": head_fields(head)["ident"],
                "recog": head_fields(head)["recog"],
                "param": head_fields(head)["param"],
                "tag": head_fields(head)["tag"],
                "series": head_fields(head)["series"],
                "body_enc": rest.decode("latin1"),
                "body_plain_hex": "",
                "body_plain_gbk": "",
            }
            if segs is not None:
                # 已知分段布局：各段独立解码后拼接
                plain = b""
                pos = 0
                ok = True
                for seg_len in segs:
                    enc_len = seg_len + (seg_len + 2) // 3
                    if pos + enc_len > len(rest):
                        ok = False
                        break
                    plain += edcode_decode(rest[pos : pos + enc_len])
                    pos += enc_len
                if ok and pos == len(rest):
                    rec["body_plain_hex"] = plain.hex()
                    rec["body_plain_gbk"] = plain.decode("gbk", "replace")
                    rec["body_segments"] = segs
            if not rec["body_plain_hex"]:
                plain = edcode_decode(rest)
                rec["body_plain_hex"] = plain.hex()
                rec["body_plain_gbk"] = plain.decode("gbk", "replace")
                rec["body_segments"] = ["flat"]
            recs.append(rec)
        out.append({"call": name, "frames": recs, "leftover_hex": leftover.hex()})
    s.close()
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", required=True, help="含 LoginSrv.exe 的目录")
    ap.add_argument("--out", required=True)
    ap.add_argument("--port", type=int, default=5599)
    ap.add_argument("--srv-port", type=int, default=5699)
    ap.add_argument(
        "--dsn",
        default="server=127.0.0.1;uid=root;pwd=;database=mir2_account;",
    )
    ap.add_argument("--server-name", default="热血传奇")
    ap.add_argument("--account", default="loadbot1")
    ap.add_argument("--password", default="loadbot1")
    ap.add_argument("--rich-account", default="mir2test", help="UserName 为空 → NEEDUPDATE 路径")
    ap.add_argument("--rich-password", default="mir2pass")
    ap.add_argument("--keep", action="store_true", help="保留临时工作目录")
    args = ap.parse_args()

    exe = os.path.join(args.src, "LoginSrv.exe")
    if not os.path.exists(exe):
        print(f"找不到 {exe}", file=sys.stderr)
        return 2

    work = tempfile.mkdtemp(prefix="a2m1-loginsrv-oracle-")
    try:
        # 复制整个发布目录（exe 按 AppContext.BaseDirectory 读配置）
        for name in os.listdir(args.src):
            sp = os.path.join(args.src, name)
            if os.path.isdir(sp):
                continue
            shutil.copy2(sp, os.path.join(work, name))
        os.makedirs(os.path.join(work, "logs"), exist_ok=True)
        write_workdir(work, args.port, args.srv_port, args.dsn, args.server_name)

        out_log = open(os.path.join(work, "stdout.log"), "wb")
        # 必须跑 work 目录里的副本：C# 按 AppContext.BaseDirectory（= exe 所在目录）读
        # logsrv.conf / AddrTable.txt —— 直接跑 --src 里那份会读到源目录的部署配置。
        proc = subprocess.Popen(
            [os.path.join(work, "LoginSrv.exe")],
            cwd=work,
            stdout=out_log,
            stderr=subprocess.STDOUT,
            creationflags=0x08000000,  # CREATE_NO_WINDOW
        )
        try:
            if not wait_port(args.port):
                print("LoginSrv 未监听，日志尾部：", file=sys.stderr)
                print(open(os.path.join(work, "stdout.log"), "rb").read()[-2000:], file=sys.stderr)
                return 3

            server_ok = args.server_name
            sname_frame = server_ok.encode("gbk")
            bad_frame = client_frame(
                2001, f"{args.account}/wrong-password".encode("ascii")
            )
            cases = {
                "protocol": [
                    ("cm_protocol", client_frame(2000, recog=20011006), None)
                ],
                "login_ok": [
                    (
                        "cm_idpassword",
                        client_frame(
                            2001, f"{args.account}/{args.password}".encode("ascii")
                        ),
                        None,
                    )
                ],
                "select_server": [
                    (
                        "cm_idpassword",
                        client_frame(
                            2001, f"{args.account}/{args.password}".encode("ascii")
                        ),
                        None,
                    ),
                    ("cm_selectserver", client_frame(104, sname_frame), None),
                ],
                "login_bad_password": [("cm_idpassword_bad", bad_frame, None)],
                "login_unknown_account": [
                    ("cm_idpassword_unknown", client_frame(2001, b"nosuchacct/x"), None)
                ],
                "login_needupdate": [
                    (
                        "cm_idpassword_mir2test",
                        client_frame(
                            2001,
                            f"{args.rich_account}/{args.rich_password}".encode("ascii"),
                        ),
                        [16, 148],
                    )
                ],
            }
            results = {}
            sid = 1
            for name, calls in cases.items():
                try:
                    results[name] = session(args.port, str(sid), calls)
                except Exception as exc:  # noqa: BLE001
                    results[name] = [{"call": "error", "error": repr(exc)}]
                sid += 1
                time.sleep(0.2)

            doc = {
                "tool": "oracle_capture.py",
                "note": "C# LoginSrv (src/LoginSrv/**) 真实响应字节；供 bins/login-srv 字面量断言",
                "gate_port": args.port,
                "srv_port": args.srv_port,
                "server_name": args.server_name,
                "account": args.account,
                "rich_account": args.rich_account,
                "cases": results,
            }
            with open(args.out, "w", encoding="utf-8") as f:
                json.dump(doc, f, ensure_ascii=False, indent=2, sort_keys=True)
            for name, calls in results.items():
                for call in calls:
                    for fr in call.get("frames", []):
                        print(
                            f"{name:24s} {call['call']:24s} ident={fr.get('ident')} "
                            f"recog={fr.get('recog')} series={fr.get('series')} "
                            f"tail={fr.get('tail')} body={fr.get('body_plain_gbk')!r}"
                        )
                        print(f"    raw={fr.get('raw_hex')}")
            print(f"\n写入 {args.out}")
            return 0
        finally:
            proc.terminate()
            try:
                proc.wait(5)
            except subprocess.TimeoutExpired:
                proc.kill()
            out_log.close()
    finally:
        if args.keep:
            print(f"保留工作目录 {work}")
        else:
            shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
